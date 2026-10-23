#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

from contextlib import ExitStack, redirect_stderr, redirect_stdout
import fcntl
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import prod_deployment as deploy_prod  # noqa: E402
import release_reconciler as releases  # noqa: E402


class ReconciliationWatchTests(unittest.TestCase):
    running = "activating\tsuccess\tPreparing cycle products\n"
    succeeded = "inactive\tsuccess\tRelease reconciliation complete\n"

    @staticmethod
    def disconnected() -> subprocess.CalledProcessError:
        return subprocess.CalledProcessError(
            255, ["ssh"], output="ssh: Could not resolve hostname prod: Temporary failure in name resolution\n",
        )

    def watch(self, statuses, *, budget=12, start_error=None,
              diagnostics_error=None, cleanup_error=None, force=None):
        self.now = 0.0
        self.calls = []
        self.messages = []
        self.stderr = io.StringIO()
        statuses = iter(statuses)

        def sleep(seconds):
            self.now += seconds

        def ssh(_config, command, **kwargs):
            self.calls.append((command, kwargs))
            if "start --no-block" in command:
                if start_error:
                    raise start_error
            elif "systemctl show" in command:
                value = next(statuses)
                if isinstance(value, tuple):
                    elapsed, value = value
                    self.assertLessEqual(elapsed, kwargs["timeout_seconds"])
                    self.now += elapsed
                if isinstance(value, BaseException):
                    raise value
                return subprocess.CompletedProcess([], 0, value)
            elif "journalctl" in command:
                if diagnostics_error:
                    raise diagnostics_error
            elif command == "systemctl unset-environment AEROBAG_FORCE_PRODUCTION_TAG":
                if cleanup_error:
                    raise cleanup_error
            else:
                self.fail(f"unexpected command: {command}")
            return subprocess.CompletedProcess([], 0, "")

        with (
            mock.patch.object(deploy_prod, "run_ssh", side_effect=ssh),
            mock.patch.object(deploy_prod.time, "monotonic", side_effect=lambda: self.now),
            mock.patch.object(deploy_prod.time, "sleep", side_effect=sleep),
            mock.patch.object(deploy_prod, "RECONCILIATION_STATUS_RECOVERY_SECONDS", budget),
            redirect_stderr(self.stderr),
        ):
            deploy_prod.run_release_reconciliation(
                deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG),
                progress=self.messages.append, dry_run=False, force_production_tag=force,
            )

    def assert_started_once(self):
        self.assertEqual(sum("start --no-block" in command for command, _ in self.calls), 1)

    def test_dns_and_timeout_recovery_retries_only_the_read(self):
        self.watch([self.running, self.disconnected(), subprocess.TimeoutExpired("ssh", 7), self.succeeded])
        self.assert_started_once()
        polls = [kwargs for command, kwargs in self.calls if "systemctl show" in command]
        self.assertEqual(len(polls), 4)
        self.assertTrue(all(poll["capture"] for poll in polls))
        self.assertEqual(sum("connection lost" in message for message in self.messages), 1)
        self.assertEqual(sum("connection restored" in message for message in self.messages), 1)
        self.assertEqual(self.messages[-1], "Release reconciliation complete")

    def test_persistent_dns_failure_has_a_deadline_and_unknown_outcome(self):
        with self.assertRaisesRegex(deploy_prod.ReleaseReconciliationStatusUnknown, "within 12s") as raised:
            self.watch([self.disconnected()] * 3)
        self.assert_started_once()
        self.assertEqual(self.now, 12)
        self.assertEqual([kw["timeout_seconds"] for cmd, kw in self.calls if "systemctl show" in cmd], [12, 7, 2])
        self.assertIn("Temporary failure in name resolution", str(raised.exception))
        self.assertIn("not canceled", str(raised.exception))
        self.assertIn("--qualification-status --watch", str(raised.exception))
        self.assertIn("--reconcile", str(raised.exception))

    def test_hung_polls_are_bounded_by_both_request_and_recovery_budgets(self):
        with self.assertRaises(deploy_prod.ReleaseReconciliationStatusUnknown):
            self.watch([
                (30, subprocess.TimeoutExpired("ssh", 30)),
                (5, subprocess.TimeoutExpired("ssh", 5)),
            ], budget=40)
        self.assertEqual(self.now, 40)
        self.assertEqual([kw["timeout_seconds"] for cmd, kw in self.calls if "systemctl show" in cmd], [30, 5])
        self.assert_started_once()

    def test_successful_read_resets_the_outage_budget(self):
        self.watch([
            self.disconnected(), self.disconnected(), self.running,
            self.disconnected(), self.disconnected(), self.succeeded,
        ])
        self.assertGreater(self.now, 12)
        self.assert_started_once()
        self.assertEqual(sum("connection restored" in message for message in self.messages), 2)

    def test_remote_probe_error_or_malformed_response_is_unknown_without_retry(self):
        for response in [subprocess.CalledProcessError(1, ["ssh"], output="probe broken"), "bad status"]:
            with self.subTest(response=response), self.assertRaises(deploy_prod.ReleaseReconciliationStatusUnknown):
                self.watch([response])
            self.assertEqual(len(self.calls), 2)
            self.assertEqual(self.now, 0)

    def test_unacknowledged_start_is_not_repeated_or_assumed_successful(self):
        for error in [self.disconnected(), subprocess.TimeoutExpired("ssh", 30)]:
            with self.subTest(error=error), self.assertRaisesRegex(
                deploy_prod.ReleaseReconciliationStatusUnknown, "start command was not repeated",
            ):
                self.watch([], start_error=error)
            self.assertEqual(len(self.calls), 1)

    def test_remote_start_rejection_remains_a_failure(self):
        error = subprocess.CalledProcessError(1, ["ssh"])
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            self.watch([], start_error=error)
        self.assertIs(raised.exception, error)
        self.assertEqual(len(self.calls), 1)

    def test_confirmed_service_failure_survives_lost_diagnostics(self):
        for error in [None, self.disconnected(), subprocess.TimeoutExpired("ssh", 30)]:
            with self.subTest(error=error), self.assertRaisesRegex(
                deploy_prod.ReleaseReconciliationFailed, "service state: failed; result: exit-code",
            ):
                self.watch([self.disconnected(), "failed\texit-code\tBuild failed\n"], diagnostics_error=error)
            self.assert_started_once()
            self.assertEqual(sum("journalctl" in cmd for cmd, _ in self.calls), 1)

    def test_failed_force_cleanup_preserves_unknown_or_interrupted_outcome(self):
        for responses, expected in [
            ([self.disconnected()] * 3, deploy_prod.ReleaseReconciliationStatusUnknown),
            ([KeyboardInterrupt()], KeyboardInterrupt),
        ]:
            with self.subTest(expected=expected), self.assertRaises(expected):
                self.watch(responses, cleanup_error=self.disconnected(), force="2026-10-11.5")
            self.assertIn("controller cleanup could not complete", self.stderr.getvalue())
            self.assertEqual(self.calls[-1][0], "systemctl unset-environment AEROBAG_FORCE_PRODUCTION_TAG")


class ProductPublicationTests(unittest.TestCase):
    def test_read_only_ssh_can_have_a_bounded_process_timeout(self) -> None:
        with (
            mock.patch.object(subprocess, "run", side_effect=subprocess.TimeoutExpired("ssh", 30)) as run,
            redirect_stdout(io.StringIO()),
            self.assertRaises(subprocess.TimeoutExpired),
        ):
            deploy_prod.run_ssh(
                {"ssh_user": "root", "ssh_host": "prod"}, "cat state.json",
                capture=True, timeout_seconds=30,
            )
        self.assertEqual(run.call_args.kwargs["timeout"], 30)

    def test_command_log_hides_captured_ssh_trace_and_records_output(self) -> None:
        config = {"ssh_user": "root", "ssh_host": "prod"}
        completed = subprocess.CompletedProcess(
            args=["ssh"], returncode=0, stdout="captured output\n", stderr=None
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            log_path = Path(temp_dir) / "commands.log"
            stdout = io.StringIO()
            with (
                mock.patch.dict(
                    os.environ,
                    {deploy_prod.COMMAND_LOG_ENV: str(log_path)},
                ),
                mock.patch.object(subprocess, "run", return_value=completed),
                redirect_stdout(stdout),
            ):
                deploy_prod.run_ssh(config, "printf secret", capture=True)

            log = log_path.read_text(encoding="utf-8")
            self.assertEqual(stdout.getvalue(), "")
            self.assertIn("ssh -o BatchMode=yes root@prod", log)
            self.assertIn("printf secret", log)
            self.assertIn("captured output", log)

    def test_deployment_module_has_no_independent_cli(self) -> None:
        self.assertFalse(hasattr(deploy_prod, "parse_args"))
        self.assertFalse(hasattr(deploy_prod, "main"))

    def test_idle_probe_reports_scheduled_refresh_progress(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        output = (
            f"{deploy_prod.RECONCILIATION_BUSY_MARKER}\tservice\tactivating\ttimer\t"
            "Preparing cycle products for 2026-09-05.1\n"
        )
        failure = subprocess.CalledProcessError(
            deploy_prod.RECONCILIATION_BUSY_EXIT_CODE,
            ["ssh"],
            output=output,
        )
        with mock.patch.object(
            deploy_prod, "run_ssh", side_effect=failure
        ) as run_ssh:
            with self.assertRaises(deploy_prod.ReleaseReconciliationBusy) as raised:
                deploy_prod.assert_release_reconciliation_idle(
                    config, dry_run=False
                )

        self.assertIn("automatic scheduled product refresh", str(raised.exception))
        self.assertIn(
            "Current progress: Preparing cycle products for 2026-09-05.1",
            str(raised.exception),
        )
        self.assertTrue(run_ssh.call_args.kwargs["capture"])
        probe = run_ssh.call_args.args[1]
        self.assertIn("ExecMainStartTimestamp", probe)
        self.assertIn("LastTriggerUSec", probe)
        self.assertIn("release-reconciliation-progress", probe)

    def test_idle_probe_does_not_mislabel_unrelated_ssh_failure(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        failure = subprocess.CalledProcessError(255, ["ssh"], output="network down\n")
        with mock.patch.object(deploy_prod, "run_ssh", side_effect=failure):
            with self.assertRaises(subprocess.CalledProcessError) as raised:
                deploy_prod.assert_release_reconciliation_idle(
                    config, dry_run=False
                )

        self.assertEqual(raised.exception.returncode, 255)

    def test_idle_probe_bounds_its_ssh_process_with_the_supplied_budget(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as ssh:
            deploy_prod.assert_release_reconciliation_idle(config, dry_run=False, timeout_seconds=7)
        self.assertEqual(ssh.call_args.kwargs["timeout_seconds"], 7)

    def test_live_feed_publication_path_matches_current_contract(self) -> None:
        self.assertEqual(deploy_prod.LIVE_FEEDS_CONTRACT_PATH, "v3")
        core_contract = (
            deploy_prod.REPO_ROOT
            / "ui/core-rust/crates/app-core/src/live_feeds.rs"
        ).read_text(encoding="utf-8")
        self.assertIn(
            f'LIVE_FEEDS_BASE_PATH: &str = "/live-feeds/{deploy_prod.LIVE_FEEDS_CONTRACT_PATH}"',
            core_contract,
        )

    def test_release_live_feed_mutable_state_is_explicitly_release_scoped(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        unit = deploy_prod.release_live_feeds_unit(config)

        self.assertIn(
            '--tfr-detail-backfill-state-root '
            '"$AEROBAG_RELEASE_LIVE_FEEDS_STATE_ROOT/tfr-detail-backfill"',
            unit,
        )
        self.assertIn(
            '--nms-notams-state-root '
            '"$AEROBAG_RELEASE_LIVE_FEEDS_STATE_ROOT/nms-notams"',
            unit,
        )
        self.assertIn(
            '--product-artifacts "$AEROBAG_RELEASE_PRODUCT_ARTIFACTS"',
            unit,
        )

    def test_deployment_runs_the_desired_state_reconciler(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        script = deploy_prod.build_product_script(config)

        self.assertIn('python3 "$SOURCE_ROOT/tools/preprocessor_tool_cache.py"', script)
        self.assertIn('--root "$ARTIFACT_ROOT/preprocessor-tools"', script)
        self.assertIn('--ref "$CONTROLLER_REV" --release', script)
        self.assertIn('flock -s 9', script)
        self.assertNotIn("aerobag-ensure-toolchain", script)
        self.assertNotIn('install -m 0755 "$CARGO_TARGET_DIR/release/preprocessor-cli"', script)
        self.assertIn("tools/reconcile_prod_releases.py", script)
        self.assertIn(
            '--controller-preprocessor "$CONTROLLER_PREPROCESSOR"',
            script,
        )
        self.assertIn("--desired \"$SOURCE_ROOT/deploy/releases.json\"", script)
        self.assertIn(
            '--legacy-deployed-rev-file "$ARTIFACT_ROOT/state/legacy-deployed-rev"',
            script,
        )
        self.assertIn(
            'force_args=(--force-production-tag "$AEROBAG_FORCE_PRODUCTION_TAG")',
            script,
        )
        self.assertIn('"${force_args[@]}"', script)
        self.assertNotIn("build_multi_version_publication.py", script)
        self.assertLess(
            script.index("Preparing release tooling"),
            script.index("tools/preprocessor_tool_cache.py"),
        )
        self.assertLess(
            script.index('"$SOURCE_ROOT/tools/reconcile_prod_releases.py"'),
            script.index(deploy_prod.CARGO_TARGET_PRUNE_SCRIPT),
        )
        self.assertLess(
            script.index(deploy_prod.CARGO_TARGET_PRUNE_SCRIPT),
            script.index("/usr/local/bin/aerobag-write-health"),
        )

    def test_cargo_target_is_bounded_on_the_data_volume(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)

        self.assertEqual(
            config["cargo_target_dir"],
            "/mnt/aerobag-data/build-cache/cargo-target",
        )
        self.assertEqual(config["cargo_target_max_bytes"], 32 * 1024**3)
        env = deploy_prod.env_file(config)
        self.assertIn(
            "CARGO_TARGET_DIR=/mnt/aerobag-data/build-cache/cargo-target\n",
            env,
        )
        self.assertIn(
            "AEROBAG_CARGO_TARGET_MAX_BYTES=34359738368\n",
            env,
        )

    def test_cargo_target_must_be_below_data_root(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        config["cargo_target_dir"] = "/var/cache/aerobag-build/target"

        with self.assertRaisesRegex(SystemExit, "child of data_root"):
            deploy_prod.validate_build_cache_config(config)

    def test_cargo_target_prune_preserves_profile_binaries(self) -> None:
        self.check_cargo_target_prune(limit_delta=-1)

    def test_cargo_target_at_limit_is_not_pruned(self) -> None:
        self.check_cargo_target_prune(limit_delta=0)

    def test_cargo_target_below_limit_is_not_pruned(self) -> None:
        self.check_cargo_target_prune(limit_delta=1)

    def check_cargo_target_prune(self, *, limit_delta: int) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            data_root = Path(temp_dir) / "data"
            target = data_root / "build-cache/cargo-target"
            deps = target / "release/deps"
            deps.mkdir(parents=True)
            dependency = deps / "libold-hash.rlib"
            # Allocate real, non-sparse, incompressible data so deleting this
            # fixture releases space independently of directory/block overhead.
            dependency_bytes = os.urandom(64 * 1024)
            dependency.write_bytes(dependency_bytes)
            binary = target / "release/preprocessor-cli"
            binary.write_bytes(b"binary")
            before = int(subprocess.check_output(
                ["du", "-sx", "--block-size=1", "--", str(target)], text=True,
            ).split()[0])
            self.assertGreater(dependency.stat().st_blocks, 0)
            # Match the production script's allocated-byte accounting; tiny
            # files/directories need not occupy the same space on every FS.
            limit = before + limit_delta
            env_path = Path(temp_dir) / "env"
            env_path.write_text(
                "".join(
                    (
                        f"DATA_ROOT={deploy_prod.shell_quote(str(data_root))}\n",
                        f"CARGO_TARGET_DIR={deploy_prod.shell_quote(str(target))}\n",
                        f"AEROBAG_CARGO_TARGET_MAX_BYTES={limit}\n",
                    )
                ),
                encoding="utf-8",
            )
            script = deploy_prod.prune_cargo_target_script().replace(
                "source /etc/aerobag/env",
                f"source {deploy_prod.shell_quote(str(env_path))}",
            )

            subprocess.run(["bash"], input=script, text=True, check=True)

            if limit_delta < 0:
                self.assertFalse(dependency.exists())
            else:
                self.assertEqual(dependency.read_bytes(), dependency_bytes)
            self.assertEqual(binary.read_bytes(), b"binary")

    def test_staging_qualification_browser_is_an_installed_dependency(self) -> None:
        packages = (deploy_prod.REPO_ROOT / deploy_prod.REPO_PACKAGE_MANIFEST).read_text(
            encoding="utf-8"
        ).splitlines()
        self.assertIn("google-chrome-stable", packages)
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        self.assertIn(
            "CHROME_BIN=/usr/bin/google-chrome-stable\n",
            deploy_prod.env_file(config),
        )
        self.assertIn(
            "export PATH CHROME_BIN",
            deploy_prod.build_product_script(config),
        )

    def test_google_chrome_package_source_is_installed_before_repo_packages(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as run_ssh:
            deploy_prod.install_external_package_sources(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertIn(deploy_prod.GOOGLE_CHROME_SIGNING_KEY_URL, command)
        self.assertIn(deploy_prod.GOOGLE_CHROME_APT_SOURCE, command)

    def test_nginx_serves_stable_channel_views_not_build_directories(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        nginx = deploy_prod.nginx_config(config)

        self.assertIn("/channel-current/production/web", nginx)
        self.assertIn("/channel-current/staging/packages/", nginx)
        self.assertEqual(nginx.count("default_type text/html;"), 3)
        self.assertIn("/channel-current/releases/", nginx)
        self.assertIn(
            "location = /about {\n        alias "
            f"{config['artifact_root']}/channel-current/production/web/about.html;",
            nginx,
        )
        self.assertIn(
            "location = /staging/ {\n        rewrite ^ /staging/index.html last;",
            nginx,
        )
        self.assertIn(
            "location = /staging/about {\n        alias "
            f"{config['artifact_root']}/channel-current/staging/web/about.html;",
            nginx,
        )
        self.assertIn(
            'location ~ "^/releases/([A-Za-z0-9][A-Za-z0-9._-]{0,79})/web/$" {',
            nginx,
        )
        self.assertIn(
            "rewrite ^/releases/([^/]+)/web/$ /releases/$1/web/index.html last;",
            nginx,
        )
        self.assertIn(
            'location ~ "^/releases/([A-Za-z0-9][A-Za-z0-9._-]{0,79})/web/about$" {',
            nginx,
        )
        self.assertIn(
            f"alias {config['artifact_root']}/channel-current/releases/$1/web/about.html;",
            nginx,
        )
        self.assertNotIn('location ~ "^/staging/(?:about)?$" {', nginx)
        self.assertNotIn(f"root {config['web_dist']};", nginx)

    def test_deploy_rejects_in_progress_reconciliation_instead_of_stopping_it(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as run_ssh:
            deploy_prod.quiesce_release_reconciliation(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertIn("systemctl stop \"$timer\"", command)
        self.assertIn("release reconciliation is already running", command)
        self.assertIn("systemctl start \"$timer\"", command)
        self.assertNotIn("systemctl stop aerobag-build-product.service", command)

    def test_stale_unit_cleanup_never_terminates_release_reconciliation(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as run_ssh:
            deploy_prod.stop_stale_units(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertNotIn("aerobag-build-product.service", command)
        self.assertNotIn("aerobag-build-product.timer", command)

    def test_managed_checkout_discards_local_edits_before_switching_refs(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as run_ssh:
            deploy_prod.install_repo_from_bundle(
                config, "/tmp/deployment.bundle", dry_run=False
            )

        command = run_ssh.call_args.args[1]
        reset = f"git -C {config['source_root']} reset --hard HEAD"
        fetch = f"git -C {config['source_root']} fetch --prune"
        self.assertLess(command.index(reset), command.index(fetch))
        self.assertIn(f"git -C {config['source_root']} clean -fd", command)
        self.assertIn(
            f"git -C {config['source_root']} checkout --detach --force main",
            command,
        )
        self.assertNotIn("checkout --detach HEAD", command)

    def test_runtime_repair_reads_remote_provider_bindings_under_the_controller_lock(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(
                deploy_prod,
                "publication_refs",
                side_effect=AssertionError("desired publication tags do not own active daemons"),
            ),
            mock.patch.object(deploy_prod, "run_ssh") as run_ssh,
        ):
            deploy_prod.start_release_live_feeds(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertIn("deployed_live_feed_targets(root)", command)
        self.assertIn("release-reconciler.lock", command)
        self.assertIn("fcntl.LOCK_EX | fcntl.LOCK_NB", command)
        self.assertIn(config["artifact_root"], command)
        self.assertNotIn("publication_refs", command)

    def test_managed_release_deploy_wraps_controller_with_runtime_services(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(deploy_prod, "run_ssh") as run_ssh,
            mock.patch.object(
                deploy_prod, "run_release_reconciliation"
            ) as reconcile,
        ):
            deploy_prod.start_reconciled_runtime(config, dry_run=False)

        self.assertEqual(run_ssh.call_count, 2)
        reconcile.assert_called_once_with(config, progress=None, dry_run=False)

    def test_release_reconciliation_relays_changed_progress_only(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        completed = subprocess.CompletedProcess([], 0, "", None)
        statuses = iter(
            [
                subprocess.CompletedProcess(
                    [], 0, "activating\tsuccess\tPreparing release tooling\n", None
                ),
                subprocess.CompletedProcess(
                    [], 0, "activating\tsuccess\tPreparing release tooling\n", None
                ),
                subprocess.CompletedProcess(
                    [], 0, "active\tsuccess\tBuilding release 2026-08-23.1\n", None
                ),
                subprocess.CompletedProcess(
                    [], 0, "inactive\tsuccess\tRelease reconciliation complete\n", None
                ),
            ]
        )

        def run_ssh(*_args, capture: bool = False, **_kwargs):
            return next(statuses) if capture else completed

        messages: list[str] = []
        with (
            mock.patch.object(deploy_prod, "run_ssh", side_effect=run_ssh),
            mock.patch.object(deploy_prod.time, "sleep"),
        ):
            deploy_prod.run_release_reconciliation(
                config, progress=messages.append, dry_run=False
            )

        self.assertEqual(
            messages,
            [
                "Preparing release tooling",
                "Building release 2026-08-23.1",
                "Release reconciliation complete",
            ],
        )

    def test_forced_reconciliation_scopes_and_clears_exact_tag_override(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        completed = subprocess.CompletedProcess([], 0, "", None)
        succeeded = subprocess.CompletedProcess(
            [], 0, "inactive\tsuccess\tRelease reconciliation complete\n", None
        )

        def run_ssh(*_args, capture: bool = False, **_kwargs):
            return succeeded if capture else completed

        with mock.patch.object(deploy_prod, "run_ssh", side_effect=run_ssh) as ssh:
            deploy_prod.run_release_reconciliation(
                config,
                dry_run=False,
                force_production_tag="2026-08-23.1",
            )

        commands = [call.args[1] for call in ssh.call_args_list]
        self.assertIn(
            "systemctl set-environment "
            "AEROBAG_FORCE_PRODUCTION_TAG=2026-08-23.1",
            commands[0],
        )
        self.assertEqual(
            commands[-1],
            "systemctl unset-environment AEROBAG_FORCE_PRODUCTION_TAG",
        )

    def test_promotion_only_syncs_intent_and_starts_release_controller(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(deploy_prod, "assert_local_refs_exist"),
            mock.patch.object(deploy_prod, "assert_clean_checkout"),
            mock.patch.object(deploy_prod, "quiesce_release_reconciliation") as quiesce,
            mock.patch.object(deploy_prod, "sync_source_checkout") as sync_source,
            mock.patch.object(
                deploy_prod, "run_release_reconciliation"
            ) as reconcile,
            mock.patch.object(deploy_prod, "run_ssh") as run_ssh,
            mock.patch.object(deploy_prod, "install_repo_packages") as install_packages,
            mock.patch.object(deploy_prod, "run_android_sdk_setup") as setup_android,
        ):
            deploy_prod.activate_release_intent(config)

        quiesce.assert_called_once_with(config, dry_run=False)
        sync_source.assert_called_once_with(config, dry_run=False)
        reconcile.assert_called_once_with(
            config,
            progress=None,
            dry_run=False,
            force_production_tag=None,
        )
        run_ssh.assert_called_once_with(
            config,
            "systemctl start aerobag-build-product.timer",
            dry_run=False,
            timeout_seconds=deploy_prod.RECONCILIATION_SSH_TIMEOUT_SECONDS,
        )
        install_packages.assert_not_called()
        setup_android.assert_not_called()

    def test_forced_promotion_passes_exact_tag_to_release_controller(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(deploy_prod, "assert_local_refs_exist"),
            mock.patch.object(deploy_prod, "assert_clean_checkout"),
            mock.patch.object(deploy_prod, "quiesce_release_reconciliation"),
            mock.patch.object(deploy_prod, "sync_source_checkout"),
            mock.patch.object(
                deploy_prod, "run_release_reconciliation"
            ) as reconcile,
            mock.patch.object(deploy_prod, "run_ssh"),
        ):
            deploy_prod.activate_release_intent(
                config,
                force_production_tag="2026-08-23.1",
            )

        reconcile.assert_called_once_with(
            config,
            progress=None,
            dry_run=False,
            force_production_tag="2026-08-23.1",
        )

    def test_promotion_restores_periodic_timer_when_intent_sync_fails(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(deploy_prod, "assert_local_refs_exist"),
            mock.patch.object(deploy_prod, "assert_clean_checkout"),
            mock.patch.object(deploy_prod, "quiesce_release_reconciliation"),
            mock.patch.object(
                deploy_prod,
                "sync_source_checkout",
                side_effect=RuntimeError("sync failed"),
            ),
            mock.patch.object(deploy_prod, "run_ssh") as run_ssh,
        ):
            with self.assertRaisesRegex(RuntimeError, "sync failed"):
                deploy_prod.activate_release_intent(config)

        run_ssh.assert_called_once_with(
            config,
            "systemctl start aerobag-build-product.timer",
            dry_run=False,
            timeout_seconds=deploy_prod.RECONCILIATION_SSH_TIMEOUT_SECONDS,
        )

    def test_promotion_cleanup_cannot_mask_unknown_deployment_status(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        unknown = deploy_prod.ReleaseReconciliationStatusUnknown("connection lost")
        with (
            mock.patch.object(deploy_prod, "assert_local_refs_exist"),
            mock.patch.object(deploy_prod, "assert_clean_checkout"),
            mock.patch.object(deploy_prod, "quiesce_release_reconciliation"),
            mock.patch.object(deploy_prod, "sync_source_checkout"),
            mock.patch.object(deploy_prod, "run_release_reconciliation", side_effect=unknown),
            mock.patch.object(deploy_prod, "run_ssh", side_effect=subprocess.CalledProcessError(255, ["ssh"])) as ssh,
            redirect_stderr(io.StringIO()),
            self.assertRaises(deploy_prod.ReleaseReconciliationStatusUnknown) as raised,
        ):
            deploy_prod.activate_release_intent(config)
        self.assertIs(raised.exception, unknown)
        self.assertEqual(ssh.call_count, 1)

    def test_runtime_repair_does_not_start_product_build(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as run_ssh:
            deploy_prod.start_support_runtime(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertNotIn("aerobag-build-product.service", command)
        self.assertIn("systemctl start aerobag-build-product.timer", command)


class RuntimeUpdateTests(unittest.TestCase):
    def test_reconcile_hands_off_archive_collector_before_release_build(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        calls = []
        with ExitStack() as stack:
            for name in [
                "assert_local_refs_exist", "assert_clean_checkout", "install_bootstrap_packages",
                "quiesce_release_reconciliation", "prepare_remote_paths", "ensure_legacy_channel_view",
                "migrate_cloud_storage_layout", "sync_source_checkout", "install_external_package_sources",
                "install_repo_packages", "install_android_signing_key", "install_nms_notams_credential",
                "install_weather_camera_credential", "install_cloud_server_secret", "install_cloud_server_policy",
                "write_remote_file", "write_remote_config", "run_initial_toolchain_build",
                "install_archive_tools", "run_android_sdk_setup", "stop_stale_units", "reload_services",
                "start_archive_services", "record_runtime_fingerprint",
            ]:
                stack.enter_context(mock.patch.object(
                    deploy_prod, name, side_effect=lambda *args, _name=name, **kwargs: calls.append(_name),
                ))
            stack.enter_context(mock.patch.object(deploy_prod, "local_ref_sha", return_value="a" * 40))
            def runtime_command(*args, **kwargs):
                self.assertIn("start_archive_services", calls)

            ssh = stack.enter_context(mock.patch.object(deploy_prod, "run_ssh", side_effect=runtime_command))

            def reconcile(*args, **kwargs):
                # This call can spend tens of minutes building products. A resident
                # collector must already use the newly installed telemetry contract.
                self.assertIn("start_archive_services", calls)
                calls.append("run_release_reconciliation")

            stack.enter_context(mock.patch.object(deploy_prod, "run_release_reconciliation", side_effect=reconcile))
            deploy_prod.reconcile_host(config)

        self.assertLess(calls.index("reload_services"), calls.index("start_archive_services"))
        self.assertLess(calls.index("install_archive_tools"), calls.index("start_archive_services"))
        self.assertEqual(calls.count("start_archive_services"), 1)
        self.assertEqual(calls[-1], "record_runtime_fingerprint")
        self.assertEqual(ssh.call_count, 2)

    def test_alarm_policy_and_engine_are_installed_and_runtime_fingerprinted(self) -> None:
        self.assertIn("deploy/alarm-dispositions.json", deploy_prod.RUNTIME_SOURCE_PATHS)
        self.assertIn("product/preprocessor/scripts/alarm_dispositions.py", deploy_prod.RUNTIME_SOURCE_PATHS)
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "write_remote_file") as write:
            deploy_prod.write_remote_config(config, deployed_rev="test", include_build_config=False, dry_run=True)
        files = {call.args[1]: call.args[2] for call in write.call_args_list}
        self.assertEqual(files["/etc/aerobag/alarm-dispositions.json"],
                         deploy_prod.ALARM_DISPOSITIONS_POLICY.read_text())
        self.assertEqual(files["/usr/local/bin/alarm_dispositions.py"],
                         deploy_prod.ALARM_DISPOSITIONS_HELPER.read_text())
        self.assertIn('Path("/etc/aerobag/alarm-dispositions.json")',
                      files["/usr/local/bin/aerobag-pipeline-health"])

    def test_fingerprint_tracks_runtime_inputs_but_not_app_docs_or_assignments(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in deploy_prod.RUNTIME_SOURCE_PATHS:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("initial", encoding="utf-8")
            policy = root / "policy.json"
            policy.write_text("{}", encoding="utf-8")
            config = {"cloud_server_policy_source": str(policy), "setting": 1}
            with mock.patch.object(deploy_prod, "REPO_ROOT", root):
                baseline = deploy_prod.runtime_fingerprint(config)
                for name in ["ui/app.ts", "docs/DEPLOY.md", "deploy/releases.json"]:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("unrelated change", encoding="utf-8")
                self.assertEqual(deploy_prod.runtime_fingerprint(config), baseline)
                self.assertNotEqual(deploy_prod.runtime_fingerprint({**config, "setting": 2}), baseline)
                for name in deploy_prod.RUNTIME_SOURCE_PATHS:
                    path = root / name
                    path.write_text("runtime change", encoding="utf-8")
                    self.assertNotEqual(deploy_prod.runtime_fingerprint(config), baseline, name)
                    path.write_text("initial", encoding="utf-8")
                policy.write_text('{"new":true}', encoding="utf-8")
                self.assertNotEqual(deploy_prod.runtime_fingerprint(config), baseline)

    def test_update_installs_runtime_without_building_or_changing_release_channels(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        calls = []
        with ExitStack() as stack:
            for name in [
                "assert_local_refs_exist", "assert_clean_checkout", "quiesce_release_reconciliation",
                "sync_source_checkout", "install_cloud_server_policy", "write_remote_config",
                "prepare_remote_paths", "migrate_cloud_storage_layout",
                "install_nms_notams_credential", "install_cloud_server_secret", "write_remote_file",
                "install_weather_camera_credential",
                "install_archive_tools", "start_archive_services",
                "reload_services", "start_support_runtime", "start_release_live_feeds",
                "record_runtime_fingerprint",
            ]:
                stack.enter_context(mock.patch.object(
                    deploy_prod, name, side_effect=lambda *args, _name=name, **kwargs: calls.append(_name),
                ))
            stack.enter_context(mock.patch.object(deploy_prod, "local_ref_sha", return_value="a" * 40))
            ssh = stack.enter_context(mock.patch.object(deploy_prod, "run_ssh"))
            forbidden = [stack.enter_context(mock.patch.object(deploy_prod, name)) for name in [
                "install_repo_packages", "run_initial_toolchain_build", "run_android_sdk_setup",
                "run_release_reconciliation", "start_reconciled_runtime",
            ]]
            deploy_prod.update_runtime(config)
        for method in forbidden:
            method.assert_not_called()
        self.assertLess(calls.index("quiesce_release_reconciliation"), calls.index("sync_source_checkout"))
        self.assertLess(calls.index("write_remote_config"), calls.index("start_support_runtime"))
        self.assertLess(calls.index("install_archive_tools"), calls.index("start_archive_services"))
        self.assertEqual(calls[-1], "record_runtime_fingerprint")
        ssh.assert_called_once_with(config, "systemctl start aerobag-build-product.timer", dry_run=False)

    def test_failed_update_stays_retryable_and_restores_timer(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(deploy_prod, "assert_local_refs_exist"),
            mock.patch.object(deploy_prod, "assert_clean_checkout"),
            mock.patch.object(deploy_prod, "local_ref_sha", return_value="a" * 40),
            mock.patch.object(deploy_prod, "quiesce_release_reconciliation"),
            mock.patch.object(deploy_prod, "sync_source_checkout", side_effect=RuntimeError("sync failed")),
            mock.patch.object(deploy_prod, "record_runtime_fingerprint") as record,
            mock.patch.object(deploy_prod, "run_ssh") as ssh,
        ):
            with self.assertRaisesRegex(RuntimeError, "sync failed"):
                deploy_prod.update_runtime(config)
        record.assert_not_called()
        ssh.assert_called_once_with(config, "systemctl start aerobag-build-product.timer", dry_run=False)


    def test_deployment_checks_invoke_only_python_and_restore_timer_after_failure(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        for failed in [False, True]:
            with (
                self.subTest(failed=failed),
                mock.patch.object(deploy_prod, "quiesce_release_reconciliation") as quiesce,
                mock.patch.object(deploy_prod, "run_ssh", side_effect=[RuntimeError("bad route") if failed else None, None]) as ssh,
            ):
                if failed:
                    with self.assertRaisesRegex(RuntimeError, "bad route"):
                        deploy_prod.check_active_deployments(config)
                else:
                    deploy_prod.check_active_deployments(config)
            quiesce.assert_called_once_with(config, dry_run=False)
            command = ssh.call_args_list[0].args[1]
            self.assertTrue(command.startswith("/usr/bin/python3 "))
            self.assertIn("--check-deployments-only", command)
            self.assertNotIn("--refresh-products", command)
            self.assertNotIn("aerobag-ensure-toolchain", command)
            self.assertEqual(ssh.call_args_list[-1].args[1], "systemctl start aerobag-build-product.timer")


class AndroidSigningKeyTests(unittest.TestCase):
    def test_default_key_lives_in_the_credentials_tree(self) -> None:
        self.assertEqual(
            deploy_prod.DEFAULT_ANDROID_SIGNING_SOURCE_KEYSTORE,
            Path("/root/aerobag-credentials/android/aerobag-app.keystore"),
        )

    def test_missing_key_fails_instead_of_copying_an_implicit_debug_key(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            missing = Path(temp_dir) / "missing.keystore"
            config = {
                "android_signing_source_keystore": str(missing),
                "android_signing_expected_cert_sha256": (
                    deploy_prod.ANDROID_SIGNING_EXPECTED_CERT_SHA256
                ),
            }
            with self.assertRaisesRegex(SystemExit, "missing Android signing keystore"):
                deploy_prod.ensure_local_android_signing_key(config, dry_run=False)


class NmsProductionCredentialTests(unittest.TestCase):
    def write_credential(self, **overrides: str) -> Path:
        credential = {
            "sourceEnvironment": "production",
            "apiBaseUrl": deploy_prod.NMS_PRODUCTION_API_BASE_URL,
            "tokenUrl": deploy_prod.NMS_PRODUCTION_TOKEN_URL,
            "clientId": "test-client",
            "clientSecret": "test-secret",
        }
        credential.update(overrides)
        path = Path(self.temp_dir.name) / "nms.json"
        path.write_text(json.dumps(credential), encoding="utf-8")
        return path

    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def test_accepts_production_credentials_and_endpoints(self) -> None:
        deploy_prod.validate_nms_notams_production_credential(
            self.write_credential()
        )

    def test_rejects_staging_environment(self) -> None:
        path = self.write_credential(sourceEnvironment="staging")
        with self.assertRaisesRegex(SystemExit, "sourceEnvironment"):
            deploy_prod.validate_nms_notams_production_credential(path)

    def test_rejects_staging_endpoints_even_when_labeled_production(self) -> None:
        path = self.write_credential(
            apiBaseUrl="https://api-staging.cgifederal-aim.com/nmsapi/v1",
            tokenUrl="https://api-staging.cgifederal-aim.com/v1/auth/token",
        )
        with self.assertRaisesRegex(SystemExit, "apiBaseUrl"):
            deploy_prod.validate_nms_notams_production_credential(path)

    def test_rejects_missing_secret(self) -> None:
        path = self.write_credential(clientSecret="")
        with self.assertRaisesRegex(SystemExit, "clientSecret"):
            deploy_prod.validate_nms_notams_production_credential(path)


class WeatherCameraCredentialTests(unittest.TestCase):
    def test_deploy_copies_token_privately_and_exports_only_its_remote_path(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        self.assertEqual(config["weather_camera_source"], "combined")
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "token"
            source.write_text("test-only-camera-token\n", encoding="ascii")
            config["weather_camera_token_file"] = str(source)
            with (
                mock.patch.object(deploy_prod, "run_ssh") as ssh,
                mock.patch.object(deploy_prod, "run_local") as local,
            ):
                deploy_prod.install_weather_camera_credential(config, dry_run=False)
            self.assertIn("install -d -m 0700", ssh.call_args.args[1])
            command = local.call_args.args[0]
            self.assertIn("--chmod=F600", command)
            self.assertIn(source, command)
            self.assertTrue(str(command[-1]).endswith(config["weather_camera_prod_token_file"]))
            env = deploy_prod.env_file(config)
            self.assertIn("AEROBAG_WEATHER_CAMERA_SOURCE=combined", env)
            self.assertIn(
                f"AEROBAG_WEATHER_CAMERA_TOKEN_FILE={config['weather_camera_prod_token_file']}", env
            )
            self.assertNotIn("test-only-camera-token", env + str(ssh.call_args) + str(local.call_args))

    def test_website_override_does_not_install_or_require_a_token(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        config["weather_camera_source"] = "website"
        config["weather_camera_token_file"] = "/does-not-exist/token"
        with (
            mock.patch.object(deploy_prod, "run_ssh") as ssh,
            mock.patch.object(deploy_prod, "run_local") as local,
        ):
            deploy_prod.install_weather_camera_credential(config, dry_run=False)
        ssh.assert_not_called()
        local.assert_not_called()
        self.assertIn("AEROBAG_WEATHER_CAMERA_SOURCE=website", deploy_prod.env_file(config))

    def test_missing_or_malformed_token_fails_before_copying_without_echoing_it(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "token"
            config["weather_camera_token_file"] = str(source)
            with (
                mock.patch.object(deploy_prod, "run_ssh") as ssh,
                mock.patch.object(deploy_prod, "run_local") as local,
            ):
                with self.assertRaisesRegex(SystemExit, "missing FAA weather-camera token") as error:
                    deploy_prod.install_weather_camera_credential(config, dry_run=False)
                self.assertIn("9-AJO-WCAM-ProgramOffice@faa.gov", str(error.exception))
                self.assertIn("MoU", str(error.exception))
                self.assertIn("docs/WEATHER_CAMERAS.md", str(error.exception))
                for value in ["", "Bearer test-only-token", "test-only-token\nX: injected"]:
                    source.write_text(value, encoding="ascii")
                    with self.assertRaisesRegex(SystemExit, "one raw token") as error:
                        deploy_prod.install_weather_camera_credential(config, dry_run=False)
                    self.assertNotIn("test-only-token", str(error.exception))
                ssh.assert_not_called()
                local.assert_not_called()


class AerobagCloudProductionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)

    def test_production_ports_do_not_collide(self) -> None:
        self.assertEqual(self.config["cloud_server_listen"], "127.0.0.1:8099")
        self.assertNotEqual(
            self.config["cloud_server_listen"], deploy_prod.CLIENT_DEBUG_LISTEN
        )
        self.assertNotEqual(
            self.config["cloud_server_listen"], self.config["live_feeds_listen"]
        )
        self.assertNotEqual(
            self.config["cloud_server_listen"], self.config["pipeline_health_listen"]
        )

    def test_nginx_derives_client_identity_only_from_the_known_outer_proxy(self) -> None:
        nginx = deploy_prod.nginx_config(self.config)
        self.assertIn("set_real_ip_from 10.53.0.3;", nginx)
        self.assertIn("real_ip_header Aerobag-Client-Address;", nginx)
        self.assertIn("real_ip_recursive off;", nginx)
        self.assertIn("proxy_set_header Aerobag-Client-Address $remote_addr;", nginx)
        self.assertIn("location = /cloud/v1/status", nginx)
        self.assertIn("return 404;", nginx)
        self.assertIn("location = /cloud/v1/events", nginx)
        self.assertIn("access_log off;", nginx)
        self.assertIn("location /cloud/", nginx)
        self.assertIn("proxy_buffering off;", nginx)
        self.assertIn("client_max_body_size 2097152;", nginx)

    def test_nginx_compresses_json_control_manifests_without_recompressing_payloads(self) -> None:
        nginx = deploy_prod.nginx_config(self.config)
        self.assertIn("gzip on;", nginx)
        self.assertIn("gzip_proxied any;", nginx)
        self.assertIn("gzip_types application/json;", nginx)
        self.assertIn("gzip_vary on;", nginx)
        self.assertNotIn("application/zip", nginx)
        self.assertNotIn("image/png", nginx)

    def test_cloud_service_uses_external_policy_secret_and_persistent_data(self) -> None:
        unit = deploy_prod.cloud_server_unit(self.config)
        self.assertIn("User=aerobag-cloud", unit)
        self.assertIn(' --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT"', unit)
        self.assertIn(' --policy "$AEROBAG_CLOUD_SERVER_POLICY"', unit)
        self.assertIn(' --server-secret "$AEROBAG_CLOUD_SERVER_SECRET"', unit)
        self.assertIn("ReadWritePaths=/mnt/aerobag-data/cloud-storage", unit)
        self.assertIn("CapabilityBoundingSet=\n", unit)
        self.assertIn("ProtectProc=invisible", unit)
        self.assertIn("RestrictNamespaces=true", unit)
        self.assertIn("TasksMax=256", unit)
        env = deploy_prod.env_file(self.config)
        self.assertIn("AEROBAG_CLOUD_SERVER_LISTEN=127.0.0.1:8099\n", env)
        self.assertIn(
            "AEROBAG_CLOUD_SERVER_STORAGE_ROOT=/mnt/aerobag-data/cloud-storage\n",
            env,
        )
        self.assertIn(
            "AEROBAG_CLOUD_SERVER_POLICY=/etc/aerobag/aerobag-cloud-policy.json\n",
            env,
        )

    def test_cloud_backup_is_online_hourly_and_uses_the_storage_root(self) -> None:
        unit = deploy_prod.cloud_backup_unit(self.config)
        self.assertIn("User=aerobag-cloud", unit)
        self.assertIn("aerobag-cloud-serverd\" backup-if-due", unit)
        self.assertIn(' --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT"', unit)
        self.assertIn("ReadWritePaths=/mnt/aerobag-data/cloud-storage", unit)
        timer = deploy_prod.cloud_backup_timer(self.config)
        self.assertIn("OnUnitActiveSec=15m", timer)
        self.assertIn("OnBootSec=5m", timer)

    def test_health_and_pipeline_service_include_cloud(self) -> None:
        self.assertIn("aerobag-cloud-server.service", deploy_prod.health_script())
        pipeline_unit = deploy_prod.pipeline_health_unit()
        self.assertIn(
            "After=network.target aerobag-cloud-server.service",
            pipeline_unit,
        )
        self.assertIn(
            "Wants=aerobag-cloud-server.service",
            pipeline_unit,
        )
        self.assertNotIn("aerobag-live-feeds.service", pipeline_unit)

    def test_cloud_policy_is_explicit_and_complete(self) -> None:
        policy = deploy_prod.cloud_policy(self.config)
        self.assertEqual(policy["schema_version"], 3)
        self.assertEqual(policy["storage"]["anonymous_account_quota_bytes"], 1_048_576)
        self.assertEqual(policy["storage"]["global_storage_limit_bytes"], 10 * 1024**3)
        self.assertEqual(policy["sse"]["max_connections_global"], 128)
        self.assertEqual(policy["garbage_collection"]["interval_seconds"], 3600)
        self.assertEqual(policy["backup"]["interval_seconds"], 3600)
        self.assertIn("gc_database_pause_ms_critical", policy["monitoring"])
        self.assertIn("backup_age_seconds_critical", policy["monitoring"])
        self.assertIn("backup_wal_growth_bytes_critical", policy["monitoring"])

    def test_cloud_secret_must_be_exactly_32_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            secret = Path(temp_dir) / "cloud.bin"
            secret.write_bytes(b"too short")
            config = dict(self.config)
            config["cloud_server_secret_source"] = str(secret)
            with self.assertRaisesRegex(SystemExit, "exactly 32 bytes"):
                deploy_prod.install_cloud_server_secret(config, dry_run=False)


class HealthServiceStateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.namespace = {"__name__": "health_test"}
        exec(compile(deploy_prod.health_script(), "generated-health", "exec"), self.namespace)

    def service(self, output: str) -> dict:
        with mock.patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0, output)) as run:
            result = self.namespace["service_state"]("example.service")
        command = run.call_args.args[0]
        self.assertEqual(command[:3], ["systemctl", "show", "example.service"])
        self.assertTrue(run.call_args.kwargs["check"])
        self.assertEqual(run.call_args.kwargs["timeout"], 10)
        return result

    def test_successful_idle_oneshot_keeps_result_and_timestamps(self) -> None:
        state = self.service(
            "LoadState=loaded\nActiveState=inactive\nSubState=dead\nUnitFileState=static\n"
            "Type=oneshot\nResult=success\nExecMainStatus=0\n"
            "ExecMainStartTimestamp=Mon 2026-10-12 20:44:45 UTC\n"
            "ExecMainExitTimestamp=Mon 2026-10-12 20:48:10 UTC\n"
        )
        self.assertEqual(state["active"], "inactive")
        self.assertEqual(state["enabled"], "static")
        self.assertEqual(state["type"], "oneshot")
        self.assertEqual(state["result"], "success")
        self.assertEqual(state["exit_status"], "0")
        self.assertEqual(state["last_finished_at"], "Mon 2026-10-12 20:48:10 UTC")
        self.assertNotIn("error", state)

    def test_failed_and_disabled_are_observations_not_query_failures(self) -> None:
        state = self.service(
            "LoadState=loaded\nActiveState=failed\nSubState=failed\nUnitFileState=disabled\n"
            "Type=oneshot\nResult=exit-code\nExecMainStatus=2\n"
        )
        self.assertEqual(state["active"], "failed")
        self.assertEqual(state["enabled"], "disabled")
        self.assertEqual(state["result"], "exit-code")
        self.assertEqual(state["exit_status"], "2")
        self.assertNotIn("error", state)

    def test_never_run_job_has_no_success_timestamp(self) -> None:
        state = self.service(
            "LoadState=loaded\nActiveState=inactive\nType=oneshot\nResult=success\n"
            "ExecMainStatus=0\nExecMainExitTimestamp=\n"
        )
        self.assertIsNone(state["last_finished_at"])

    def test_missing_and_unreadable_units_remain_unknown(self) -> None:
        for output in ["LoadState=not-found\nActiveState=inactive\n", ""]:
            with self.subTest(output=output):
                state = self.service(output)
                self.assertEqual(state["active"], "unknown")
                self.assertIn("error", state)
        for error in [FileNotFoundError("systemctl"), subprocess.TimeoutExpired("systemctl", 10),
                      subprocess.CalledProcessError(1, "systemctl")]:
            with self.subTest(error=error), mock.patch.object(subprocess, "run", side_effect=error):
                state = self.namespace["service_state"]("example.service")
                self.assertEqual(state["active"], "unknown")
                self.assertIn("error", state)

    def test_timer_schedule_is_attached_to_timer_and_activated_service(self) -> None:
        services = {"job.timer": {}, "job.service": {}, "other.service": {}}
        rows = [{"unit": "job.timer", "activates": "job.service", "next": 1791843295283015}]
        with mock.patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(rows))) as run:
            self.namespace["add_timer_schedules"](services)
        self.assertEqual(services["job.service"], {
            "timer": "job.timer", "next_run_at_utc": "2026-10-12T22:14:55.283015Z",
        })
        self.assertEqual(services["job.timer"], services["job.service"])
        self.assertEqual(services["other.service"], {})
        self.assertEqual(run.call_args.args[0][-1], "job.timer")

    def test_unscheduled_timer_and_unreadable_schedule_are_distinct(self) -> None:
        for value in [None, 0, 2**64 - 1]:
            with self.subTest(value=value):
                services = {"job.timer": {}, "job.service": {}}
                rows = [{"unit": "job.timer", "activates": "job.service", "next": value}]
                with mock.patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(rows))):
                    self.namespace["add_timer_schedules"](services)
                self.assertIsNone(services["job.service"]["next_run_at_utc"])
                self.assertNotIn("schedule_error", services["job.timer"])
        for output in ["not json", "{}", "[null]"]:
            services = {"job.timer": {}}
            with mock.patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0, output)):
                self.namespace["add_timer_schedules"](services)
            self.assertIn("schedule_error", services["job.timer"])
            self.assertNotIn("next_run_at_utc", services["job.timer"])

    def test_timer_query_failure_does_not_hide_service_observations(self) -> None:
        for error in [FileNotFoundError("systemctl"), subprocess.TimeoutExpired("systemctl", 10),
                      subprocess.CalledProcessError(1, "systemctl")]:
            with self.subTest(error=error):
                services = {"job.timer": {"active": "active"}, "job.service": {"active": "failed"}}
                with mock.patch.object(subprocess, "run", side_effect=error):
                    self.namespace["add_timer_schedules"](services)
                self.assertEqual(services["job.service"], {"active": "failed"})
                self.assertEqual(services["job.timer"]["active"], "active")
                self.assertIn("schedule_error", services["job.timer"])


class ResolvedLiveFeedRuntimeTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.generation = self.root / "channel-generations/00000002"
        self.generation.mkdir(parents=True)
        (self.root / "channel-current").symlink_to(self.generation, target_is_directory=True)
        self.observed = releases.ObservedState(production="prod", staging="stage", sunset=["old", "older"])
        for tag in ["prod", "stage", "old", "older", "retired"]:
            self.observed.releases[tag] = releases.ObservedRelease(tag=tag, tag_object="a" * 40, commit="b" * 40)
        self.production = self.add_instance("prod", "a", 8100)
        self.staging = self.add_instance("stage", "b", 8101)
        self.redundant = self.add_instance("old", "c", 8102, "stopped")
        self.observed.releases["old"].live_feed_provider = self.production.instance_id
        self.observed.releases["older"].live_feed_provider = self.production.instance_id
        self.add_instance("retired", "d", 8103, "removed")
        self.metadata = {"schema_version": 2, "generation": 2, "production": "prod",
                         "staging": "stage", "sunset": ["old", "older"]}
        self.bindings = {"schema_version": 1, "releases": {
            tag: {"release_tag": tag, "provider": provider.instance_id, "provider_tag": provider.release_tag,
                  "endpoint": provider.endpoint, "reason": "compatible" if tag in {"old", "older"} else "dedicated",
                  "verification": "e" * 64}
            for tag, provider in [("prod", self.production), ("stage", self.staging),
                                  ("old", self.production), ("older", self.production)]}}
        self.routes = {"schema_version": 1, "production": self.production.endpoint,
                       "staging": self.staging.endpoint, "releases": {
                           tag: binding["endpoint"] for tag, binding in self.bindings["releases"].items()}}
        self.write_state()

    def add_instance(self, tag: str, digest: str, port: int, status: str = "running") -> releases.LiveFeedInstance:
        key = f"{tag}-{digest * 16}"
        roots = [self.root / namespace / key for namespace in (
            "live-feeds/instances", "scratch/live-feeds/instances", "state/live-feeds/instances", "live-feed-launches",
        )]
        for root in roots:
            root.mkdir(parents=True)
        current = roots[0] / "v3/current.json"
        current.parent.mkdir()
        current.write_text("{}")
        manifest = roots[-1] / "packages/product_artifacts.json"
        manifest.parent.mkdir()
        manifest.write_text("{}")
        (manifest.parent / "current_artifacts.json").write_text("[]")
        instance = releases.LiveFeedInstance(
            instance_id=key, release_tag=tag, launch_digest=digest * 64,
            unit=f"aerobag-live-feeds-release@{key}.service", endpoint=f"http://127.0.0.1:{port}",
            manifest=str(manifest), roots=[str(root) for root in roots],
            gc_paths=[str((manifest.parent / "current_artifacts.json").relative_to(self.root))], status=status,
        )
        self.observed.live_feed_instances[key] = instance
        self.observed.releases[tag].live_feed_instance = key
        self.observed.releases[tag].live_feed_provider = key
        return instance

    def write_state(self) -> None:
        releases.write_observed_state(self.root / "state/releases-observed.json", self.observed)
        for name, document in [("generation.json", self.metadata), ("live-feed-bindings.json", self.bindings),
                               ("live-feed-routes.json", self.routes)]:
            (self.generation / name).write_text(json.dumps(document))

    def repair_script(self) -> str:
        config = {"source_root": str(deploy_prod.REPO_ROOT), "artifact_root": str(self.root)}
        with mock.patch.object(deploy_prod, "run_ssh") as ssh:
            deploy_prod.start_release_live_feeds(config, dry_run=False)
        return ssh.call_args.args[1].split("<<'PY'\n", 1)[1].rsplit("\nPY", 1)[0]

    def repair(self) -> None:
        with mock.patch.object(sys, "argv", ["-", str(deploy_prod.REPO_ROOT), str(self.root)]):
            exec(compile(self.repair_script(), "remote-runtime-repair", "exec"), {"__name__": "repair_test"})

    def health(self) -> tuple[dict, mock.Mock]:
        namespace = {"__name__": "health_test"}
        exec(compile(deploy_prod.health_script(), "generated-health", "exec"), namespace)
        namespace["read_env"] = lambda: {
            "SOURCE_ROOT": str(deploy_prod.REPO_ROOT), "ARTIFACT_ROOT": str(self.root), "DATA_ROOT": str(self.root),
        }
        namespace["DEPLOYED_REV_FILE"] = self.root / "absent-revision"
        namespace["DEPLOY_CONFIG_FILE"] = self.root / "absent-config"
        service = mock.Mock(return_value={"active": "active", "enabled": "enabled"})
        namespace["service_state"] = service
        namespace["add_timer_schedules"] = mock.Mock()
        self.assertEqual(namespace["main"](), 0)
        namespace["add_timer_schedules"].assert_called_once()
        return json.loads((self.root / "health/status.json").read_text()), service

    def test_repair_starts_each_bound_provider_once_without_redundant_or_retired_daemons(self) -> None:
        with mock.patch.object(deploy_prod.subprocess, "run") as run, \
             mock.patch.object(releases, "resolve_live_feed_bindings", side_effect=AssertionError("do not replan")):
            self.repair()
        run.assert_called_once_with(
            ["systemctl", "start", *sorted([self.production.unit, self.staging.unit])], check=True, timeout=60,
        )

    def test_same_tag_pending_candidate_does_not_replace_the_activated_provider(self) -> None:
        candidate = self.add_instance("prod", "f", 8110, "pending")
        self.write_state()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            self.repair()
        self.assertIn(self.production.unit, run.call_args.args[0])
        self.assertNotIn(candidate.unit, run.call_args.args[0])
        payload, _service = self.health()
        self.assertEqual(payload["live_feeds"]["current_json"], str(Path(self.production.roots[0]) / "v3/current.json"))

    def test_health_uses_instance_current_file_and_attributes_shared_consumers(self) -> None:
        payload, service = self.health()
        live = payload["live_feeds"]
        self.assertIsNone(live["error"])
        self.assertTrue(live["current_json_exists"])
        self.assertEqual(live["current_json"], str(Path(self.production.roots[0]) / "v3/current.json"))
        self.assertEqual(live["providers"][self.production.unit]["releases"], ["prod", "old", "older"])
        self.assertEqual(service.call_args_list.count(mock.call(self.production.unit)), 1)
        self.assertNotIn(mock.call(self.redundant.unit), service.call_args_list)
        self.assertNotIn("aerobag-live-feeds-release@retired.service", payload["services"])
        self.assertEqual(set(live["providers"]), {self.production.unit, self.staging.unit})

    def test_unknown_provider_blocks_all_starts_and_is_reported_without_a_guessed_health_path(self) -> None:
        del self.observed.live_feed_instances[self.production.instance_id]
        self.write_state()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            with self.assertRaisesRegex(releases.ReleaseConfigError, "missing instance"):
                self.repair()
        run.assert_not_called()
        payload, service = self.health()
        self.assertIn("missing instance", payload["live_feeds"]["error"])
        self.assertIsNone(payload["live_feeds"]["current_json"])
        self.assertEqual(payload["live_feeds"]["providers"], {})
        self.assertFalse(any("live-feeds-release@" in call.args[0] for call in service.call_args_list))

    def test_missing_required_binding_sidecar_does_not_start_tag_named_fallbacks(self) -> None:
        (self.generation / "live-feed-bindings.json").unlink()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            with self.assertRaisesRegex(releases.ReleaseConfigError, "live-feed-bindings.json"):
                self.repair()
        run.assert_not_called()

    def test_removed_active_provider_is_not_restarted(self) -> None:
        self.production.status = "removed"
        self.write_state()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            with self.assertRaisesRegex(releases.ReleaseConfigError, "has been removed"):
                self.repair()
        run.assert_not_called()

    def test_unexpected_provider_unit_is_not_executed(self) -> None:
        self.production.unit = "aerobag-live-feeds.service"
        self.write_state()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            with self.assertRaisesRegex(releases.ReleaseConfigError, "unexpected.*unit"):
                self.repair()
        run.assert_not_called()

    def test_instance_endpoint_disagreement_does_not_start_an_unverified_provider(self) -> None:
        self.production.endpoint = "http://127.0.0.1:8199"
        self.write_state()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            with self.assertRaisesRegex(releases.ReleaseConfigError, "identity disagrees"):
                self.repair()
        run.assert_not_called()

    def test_no_active_generation_does_not_guess_daemon_ownership(self) -> None:
        (self.root / "channel-current").unlink()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            with self.assertRaisesRegex(releases.ReleaseConfigError, "no active release generation"):
                self.repair()
        run.assert_not_called()

    def test_repair_refuses_to_race_the_release_reconciler(self) -> None:
        lock_path = self.root / "locks/release-reconciler.lock"
        lock_path.parent.mkdir()
        with lock_path.open("a+") as lock, mock.patch.object(deploy_prod.subprocess, "run") as run:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):
                self.repair()
        run.assert_not_called()

    def test_legacy_generation_uses_only_its_explicit_release_owned_routes(self) -> None:
        self.metadata.update(schema_version=1, staging=None, sunset=[])
        self.routes.update(staging=None, releases={"prod": self.production.endpoint})
        self.observed.releases["prod"].live_feed_instance = None
        self.observed.releases["prod"].live_feed_provider = None
        self.observed.releases["prod"].live_feed_endpoint = self.production.endpoint
        self.write_state()
        (self.generation / "live-feed-bindings.json").unlink()
        with mock.patch.object(deploy_prod.subprocess, "run") as run:
            self.repair()
        run.assert_called_once_with(["systemctl", "start", "aerobag-live-feeds-release@prod.service"],
                                    check=True, timeout=60)
        payload, _service = self.health()
        self.assertEqual(payload["live_feeds"]["current_json"], str(self.root / "live-feeds/releases/prod/v3/current.json"))


if __name__ == "__main__":
    unittest.main()
