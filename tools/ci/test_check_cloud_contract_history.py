# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from pathlib import Path
import subprocess
import tempfile
import unittest

import check_cloud_contract_history as contracts


class CloudContractHistoryTests(unittest.TestCase):
    def test_old_snapshots_cannot_be_rewritten_or_removed_but_new_versions_can_be_added(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)
            git("init", "-b", "main")
            git("config", "user.email", "test@example.invalid")
            git("config", "user.name", "Contract Test")
            path = root / contracts.CONTRACTS / "account-v2.json"
            path.parent.mkdir(parents=True)
            original = b'{"version":2}\n'
            path.write_bytes(original)
            git("add", ".")
            git("commit", "-m", "Publish contract")
            self.assertEqual(contracts.check(root), [])
            (path.parent / "account-v3.json").write_text('{"version":3}\n')
            self.assertEqual(contracts.check(root), [])
            path.write_text('{"version":2,"silently_changed":true}\n')
            self.assertEqual(len(contracts.check(root)), 1)
            git("add", ".")
            git("commit", "-m", "Illegal rewrite must fail even after commit")
            self.assertEqual(len(contracts.check(root)), 1)
            path.unlink()
            self.assertEqual(len(contracts.check(root)), 1)
            path.write_bytes(original)
            self.assertEqual(contracts.check(root), [])


if __name__ == "__main__":
    unittest.main()
