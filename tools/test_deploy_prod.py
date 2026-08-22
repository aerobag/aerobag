#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import deploy_prod  # noqa: E402


class ProductPublicationTests(unittest.TestCase):
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

    def test_deployment_runs_the_desired_state_reconciler(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        script = deploy_prod.build_product_script(config)

        self.assertIn("tools/reconcile_prod_releases.py", script)
        self.assertIn("--desired \"$SOURCE_ROOT/deploy/releases.json\"", script)
        self.assertIn(
            '--legacy-deployed-rev-file "$ARTIFACT_ROOT/state/legacy-deployed-rev"',
            script,
        )
        self.assertNotIn("build_multi_version_publication.py", script)

    def test_nginx_serves_stable_channel_views_not_build_directories(self) -> None:
        config = deploy_prod.load_config(deploy_prod.DEFAULT_CONFIG)
        nginx = deploy_prod.nginx_config(config)

        self.assertIn("/channel-current/production/web", nginx)
        self.assertIn("/channel-current/staging/packages/", nginx)
        self.assertIn("/channel-current/releases/", nginx)
        self.assertIn("/web/(?:about)?$", nginx)
        self.assertIn(
            'location ~ "^/releases/([A-Za-z0-9][A-Za-z0-9._-]{0,79})/web/(?:about)?$" {',
            nginx,
        )
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
