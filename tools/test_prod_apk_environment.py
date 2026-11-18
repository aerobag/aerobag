# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise the production APK entrypoint through its Gradle environment boundary."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ProductionApkEnvironmentTests(unittest.TestCase):
    def test_default_contract_and_explicit_empty_live_source_reach_gradle(self):
        for overrides in ({}, {"ANDROID_PACKAGE_SOURCE_BASE_URL": "https://aerobag.org/contracts/C1/packages/",
                               "ANDROID_LIVE_FEED_SOURCE_BASE_URL": ""}):
            with self.subTest(overrides=overrides), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                app = root / "ui/android-app"
                scripts = app / "scripts"
                scripts.mkdir(parents=True)
                shutil.copyfile(ROOT / "ui/android-app/scripts/build_prod_apk.sh", scripts / "build_prod_apk.sh")
                (scripts / "require_android_jdk.sh").write_text('JAVA_TOOL_OPTIONS=""\n')
                inventory = root / "crates/product-contracts/contracts"
                inventory.mkdir(parents=True)
                shutil.copyfile(ROOT / "crates/product-contracts/contracts/service-contract.json", inventory / "service-contract.json")
                # Stop exactly at the external compiler boundary, after the real
                # shipping script has applied every default and environment override.
                gradle = app / "gradlew"
                gradle.write_text('''#!/usr/bin/python3
import json,os,pathlib
pathlib.Path(os.environ["TEST_GRADLE_ENV"]).write_text(json.dumps({key:os.environ[key] for key in
 ("ANDROID_PACKAGE_SOURCE_BASE_URL", "ANDROID_LIVE_FEED_SOURCE_BASE_URL", "AEROBAG_SERVICE_BULLETIN_URLS")}))
raise SystemExit(73)
''')
                gradle.chmod(0o755)
                output = root / "gradle-env.json"
                environment = {key: value for key, value in os.environ.items()
                               if not key.startswith(("ANDROID_", "AEROBAG_", "GRADLE_"))}
                environment.update(AEROBAG_UI_TARGET_ROOT=str(root / "target"), TEST_GRADLE_ENV=str(output))
                environment.update(overrides)
                result = subprocess.run(["bash", str(scripts / "build_prod_apk.sh")], env=environment,
                                        cwd=root, text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 73, result.stderr)
                configured = json.loads(output.read_text())
                self.assertEqual(configured["ANDROID_PACKAGE_SOURCE_BASE_URL"], "https://aerobag.org/contracts/C1/packages/")
                self.assertEqual(configured["ANDROID_LIVE_FEED_SOURCE_BASE_URL"], "")
                self.assertEqual(configured["AEROBAG_SERVICE_BULLETIN_URLS"], "https://aerobag.org/service/bulletins-v2.json")
