# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import release_client_qualification as qualification


class ClientSelectionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.git("init", "-q")
        self.git("config", "user.name", "test")
        self.git("config", "user.email", "test@example.invalid")
        shutil.copytree(qualification.ROOT / qualification.service_contract.DIRECTORY,
                        self.root / qualification.service_contract.DIRECTORY)
        (self.root / "deploy").mkdir()
        self.intent = {"schema_version": 3, "production": {"tag": "previous"},
                       "staging": {"tag": "client1"}, "sunset": []}
        self.commit("client1")
        self.client = self.git("rev-parse", "HEAD")
        self.intent["production"] = {"tag": "client1"}
        self.intent["staging"] = {"tag": "server2", "client_tag": "client1"}
        self.commit("server2")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True, stderr=subprocess.DEVNULL).strip()

    def commit(self, tag=None):
        (self.root / "deploy/releases.json").write_text(json.dumps(self.intent))
        self.git("add", ".")
        self.git("commit", "-qm", "test", "--allow-empty")
        if tag:
            self.git("tag", tag)

    def test_exact_staged_server_selects_the_original_client(self):
        selected = qualification.selection(self.root)
        self.assertEqual(selected["client_commit"], self.client)
        self.assertEqual(selected["server_commit"], self.git("rev-parse", "HEAD"))
        self.assertNotEqual(selected["client_commit"], selected["server_commit"])

    def test_later_main_commit_does_not_reuse_stale_staging_assignment(self):
        self.commit()
        selected = qualification.selection(self.root)
        self.assertEqual(selected["client_commit"], selected["server_commit"])

    def test_same_named_but_changed_contract_cannot_qualify(self):
        path = self.root / qualification.service_contract.CURRENT
        descriptor = json.loads(path.read_text())
        descriptor["behavior"].append("different")
        path.write_text(json.dumps(descriptor))
        with self.assertRaises(ValueError):
            qualification.selection(self.root)
