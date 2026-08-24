#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

from contextlib import redirect_stdout
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


class ProductPublicationTests(unittest.TestCase):
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

    def test_deployment_runs_the_desired_state_reconciler(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        script = deploy_prod.build_product_script(config)

        self.assertIn(
            'install -m 0755 "$CARGO_TARGET_DIR/release/preprocessor-cli" '
            '"$CONTROLLER_TOOL_ROOT/preprocessor-cli"',
            script,
        )
        self.assertIn("tools/reconcile_prod_releases.py", script)
        self.assertIn(
            '--controller-preprocessor "$CONTROLLER_TOOL_ROOT/preprocessor-cli"',
            script,
        )
        self.assertIn("--desired \"$SOURCE_ROOT/deploy/releases.json\"", script)
        self.assertIn(
            '--legacy-deployed-rev-file "$ARTIFACT_ROOT/state/legacy-deployed-rev"',
            script,
        )
        self.assertNotIn("build_multi_version_publication.py", script)
        self.assertLess(
            script.index("Preparing release tooling"),
            script.index("/usr/local/bin/aerobag-ensure-toolchain"),
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
        with tempfile.TemporaryDirectory() as temp_dir:
            data_root = Path(temp_dir) / "data"
            target = data_root / "build-cache/cargo-target"
            deps = target / "release/deps"
            deps.mkdir(parents=True)
            dependency = deps / "libold-hash.rlib"
            dependency.write_bytes(b"dependency")
            binary = target / "release/preprocessor-cli"
            binary.write_bytes(b"binary")
            env_path = Path(temp_dir) / "env"
            env_path.write_text(
                "".join(
                    (
                        f"DATA_ROOT={deploy_prod.shell_quote(str(data_root))}\n",
                        f"CARGO_TARGET_DIR={deploy_prod.shell_quote(str(target))}\n",
                        "AEROBAG_CARGO_TARGET_MAX_BYTES=16384\n",
                    )
                ),
                encoding="utf-8",
            )
            script = deploy_prod.prune_cargo_target_script().replace(
                "source /etc/aerobag/env",
                f"source {deploy_prod.shell_quote(str(env_path))}",
            )

            subprocess.run(["bash"], input=script, text=True, check=True)

            self.assertFalse(dependency.exists())
            self.assertTrue(binary.is_file())

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
        self.assertIn("/channel-current/releases/", nginx)
        self.assertIn(
            "location = /staging/ {\n        rewrite ^ /staging/index.html last;",
            nginx,
        )
        self.assertIn(
            'location ~ "^/staging/(?:index\\.html|about)$" {', nginx
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
            'location ~ "^/releases/([A-Za-z0-9][A-Za-z0-9._-]{0,79})/web/(?:index\\.html|about)$" {',
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

    def test_runtime_repair_starts_every_desired_release_daemon(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with (
            mock.patch.object(
                deploy_prod,
                "publication_refs",
                return_value=["2026-08-20.1", "2026-08-22.1"],
            ),
            mock.patch.object(deploy_prod, "run_ssh") as run_ssh,
        ):
            deploy_prod.start_release_live_feeds(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertIn(
            "aerobag-live-feeds-release@2026-08-20.1.service", command
        )
        self.assertIn(
            "aerobag-live-feeds-release@2026-08-22.1.service", command
        )

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
        reconcile.assert_called_once_with(config, progress=None, dry_run=False)
        run_ssh.assert_called_once_with(
            config,
            "systemctl start aerobag-build-product.timer",
            dry_run=False,
        )
        install_packages.assert_not_called()
        setup_android.assert_not_called()

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
        )

    def test_runtime_repair_does_not_start_product_build(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        with mock.patch.object(deploy_prod, "run_ssh") as run_ssh:
            deploy_prod.start_support_runtime(config, dry_run=False)

        command = run_ssh.call_args.args[1]
        self.assertNotIn("aerobag-build-product.service", command)
        self.assertIn("systemctl start aerobag-build-product.timer", command)


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


if __name__ == "__main__":
    unittest.main()
