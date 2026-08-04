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


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import deploy_prod  # noqa: E402


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
        self.assertIn("location /cloud/", nginx)
        self.assertIn("proxy_buffering off;", nginx)
        self.assertIn("client_max_body_size 2097152;", nginx)

    def test_cloud_service_uses_external_policy_secret_and_persistent_data(self) -> None:
        unit = deploy_prod.cloud_server_unit(self.config)
        self.assertIn("User=aerobag-cloud", unit)
        self.assertIn(' --data-root "$AEROBAG_CLOUD_SERVER_DATA_ROOT"', unit)
        self.assertIn(' --policy "$AEROBAG_CLOUD_SERVER_POLICY"', unit)
        self.assertIn(' --server-secret "$AEROBAG_CLOUD_SERVER_SECRET"', unit)
        self.assertIn("ReadWritePaths=/mnt/aerobag-data/cloud", unit)
        env = deploy_prod.env_file(self.config)
        self.assertIn("AEROBAG_CLOUD_SERVER_LISTEN=127.0.0.1:8099\n", env)
        self.assertIn("AEROBAG_CLOUD_SERVER_DATA_ROOT=/mnt/aerobag-data/cloud\n", env)
        self.assertIn(
            "AEROBAG_CLOUD_SERVER_POLICY=/etc/aerobag/aerobag-cloud-policy.json\n",
            env,
        )

    def test_health_and_pipeline_service_include_cloud(self) -> None:
        self.assertIn("aerobag-cloud-server.service", deploy_prod.health_script())
        pipeline_unit = deploy_prod.pipeline_health_unit()
        self.assertIn(
            "After=network.target aerobag-live-feeds.service aerobag-cloud-server.service",
            pipeline_unit,
        )
        self.assertIn(
            "Wants=aerobag-live-feeds.service aerobag-cloud-server.service",
            pipeline_unit,
        )

    def test_cloud_policy_is_explicit_and_complete(self) -> None:
        policy = deploy_prod.cloud_policy(self.config)
        self.assertEqual(policy["schema_version"], 1)
        self.assertEqual(policy["storage"]["anonymous_account_quota_bytes"], 1_048_576)
        self.assertEqual(policy["storage"]["global_storage_limit_bytes"], 10 * 1024**3)
        self.assertEqual(policy["sse"]["max_connections_global"], 128)
        self.assertEqual(policy["garbage_collection"]["interval_seconds"], 3600)
        self.assertIn("gc_database_pause_ms_critical", policy["monitoring"])

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
