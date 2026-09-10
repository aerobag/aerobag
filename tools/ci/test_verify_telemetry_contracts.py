# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import copy
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_telemetry_contracts as verifier


class TelemetryVerificationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.repo = Path(temporary.name)
        self.root = self.repo / verifier.CATALOG
        self.git("init", "-q")
        self.git("commit", "--allow-empty", "-qm", "legacy producer")
        commit = self.git("rev-parse", "HEAD")
        self.git("tag", "old")
        shutil.copytree(verifier.ROOT / verifier.CATALOG, self.root)
        legacy = verifier.contracts.read_object(self.root / "legacy-releases.json")
        entry = copy.deepcopy(next(iter(legacy["releases"].values())))
        entry["tags"] = ["old"]
        legacy["releases"][commit] = entry
        self.write("legacy-releases.json", legacy)
        (self.repo / "deploy").mkdir()
        (self.repo / "deploy/releases.json").write_text(json.dumps({"production": {"tag": "old"}}))
        self.git("add", ".")
        self.git("commit", "-qm", "initial telemetry promises")
        self.base = self.git("rev-parse", "HEAD")

    def git(self, *args):
        return subprocess.check_output([
            "git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", *args,
        ], cwd=self.repo, text=True).strip()

    def write(self, name, document):
        (self.root / name).write_text(json.dumps(document, indent=2) + "\n")

    def test_unchanged_catalog_and_legacy_production_pass(self):
        verifier.verify(self.repo)

    def test_rewriting_a_descriptor_is_rejected_even_with_same_json_semantics(self):
        path = self.root / "product-facts-v1.json"
        path.write_text(path.read_text() + "\n")
        with self.assertRaisesRegex(ValueError, "immutable"):
            verifier.verify(self.repo)

    def test_legacy_registry_is_frozen_not_a_future_downgrade_escape_hatch(self):
        path = self.root / "legacy-releases.json"
        path.write_text(path.read_text() + "\n")
        with self.assertRaisesRegex(ValueError, "immutable"):
            verifier.verify(self.repo)

    def test_changing_current_pins_and_baseline_together_cannot_hide_removed_coverage(self):
        policy = verifier.contracts.read_object(self.root / "coverage-policy.json")
        legacy = verifier.contracts.read_object(self.root / "legacy-releases.json")
        old = next(iter(legacy["releases"].values()))["producers"]
        self.write("producers.json", {"schema_version": 1, "producers": old})
        policy["required"] = old
        self.write("coverage-policy.json", policy)
        with self.assertRaisesRegex(ValueError, "coverage removed"):
            verifier.verify(self.repo)
        self.git("add", ".")
        self.git("commit", "-qm", "unreviewed removal")
        with self.assertRaisesRegex(ValueError, "coverage removed"):
            verifier.verify(self.repo, self.base)

    def test_removing_a_monitoring_rule_fails_even_when_contract_is_unchanged(self):
        with patch.dict(verifier.pipeline_health.PRODUCT_METRIC_REQUIREMENTS, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "no monitoring rules"):
                verifier.verify(self.repo)

    def test_missing_integration_base_is_not_silently_skipped(self):
        with self.assertRaises(subprocess.CalledProcessError):
            verifier.verify(self.repo, "missing-base")


if __name__ == "__main__":
    unittest.main()
