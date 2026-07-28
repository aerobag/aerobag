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


if __name__ == "__main__":
    unittest.main()
