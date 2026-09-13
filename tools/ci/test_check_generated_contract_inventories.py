# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from contextlib import redirect_stdout
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import check_generated_contract_inventories as inventories
import check_generated_ui_sources


class GeneratedContractInventoriesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.generated = self.root / "generated"
        self.checked_in = self.root / inventories.CONTRACTS
        self.generated.mkdir()
        self.checked_in.mkdir(parents=True)
        for name in inventories.OUTPUTS:
            (self.generated / name).write_bytes(b'{"synthetic":true}\n')
            (self.checked_in / name).write_bytes(b'{"synthetic":true}\n')

    def test_exact_outputs_match_without_rewriting_checked_in_files(self):
        self.assertEqual(inventories.compare(self.generated, self.checked_in), [])

    def test_missing_live_feed_generator_output_is_not_silently_ignored(self):
        (self.generated / "live-feed-compatibility.json").unlink()
        self.assertEqual(inventories.compare(self.generated, self.checked_in), [
            "generator omitted required inventory: live-feed-compatibility.json",
        ])

    def test_both_files_missing_and_absent_output_directory_fail(self):
        for name in inventories.OUTPUTS:
            (self.generated / name).unlink()
            (self.checked_in / name).unlink()
        self.generated.rmdir()
        errors = inventories.compare(self.generated, self.checked_in)
        self.assertEqual(len(errors), 4)
        for name in inventories.OUTPUTS:
            self.assertIn(f"generator omitted required inventory: {name}", errors)
            self.assertIn(f"checked-in inventory is missing: {self.checked_in / name}", errors)

    def test_missing_checked_in_inventory_is_not_regenerated_into_a_pass(self):
        path = self.checked_in / "live-feed-compatibility.json"
        path.unlink()
        self.assertEqual(inventories.compare(self.generated, self.checked_in), [
            f"checked-in inventory is missing: {path}",
        ])
        self.assertFalse(path.exists())

    def test_stale_compatibility_is_rejected_without_repair(self):
        path = self.checked_in / "live-feed-compatibility.json"
        path.write_bytes(b"stale")
        self.assertEqual(inventories.compare(self.generated, self.checked_in), [
            f"checked-in inventory is stale: {path}",
        ])
        self.assertEqual(path.read_bytes(), b"stale")

    def test_unexpected_generated_inventory_cannot_bypass_explicit_roster(self):
        (self.generated / "unexpected.json").write_bytes(b"{}")
        self.assertEqual(inventories.compare(self.generated, self.checked_in), [
            "generator produced unexpected inventory member: unexpected.json",
        ])

    def test_standalone_command_uses_locked_rust_exporter_and_empty_artifacts(self):
        def generate(command, *, cwd, check, env):
            self.assertEqual(command[:6], ["cargo", "+1.94.1", "run", "--quiet", "--locked", "--manifest-path"])
            self.assertEqual(command[6], str(self.root / "crates/Cargo.toml"))
            self.assertEqual(command[7:13], ["-p", "product-contracts", "--bin", "export-contract-inventory", "--", "--output-dir"])
            self.assertEqual(cwd, self.root)
            self.assertTrue(check)
            output = Path(command[-1])
            self.assertFalse(output.is_relative_to(self.root))
            self.assertEqual(list(Path(env["AEROBAG_ARTIFACT_READ_PATH"]).iterdir()), [])
            output.mkdir()
            for name in inventories.OUTPUTS:
                (output / name).write_bytes((self.generated / name).read_bytes())

        with mock.patch.object(inventories.subprocess, "run", side_effect=generate) as run:
            self.assertEqual(inventories.check(self.root), [])
        run.assert_called_once()

    def test_successful_but_incomplete_generator_fails_the_entrypoint(self):
        def generate(command, **_kwargs):
            output = Path(command[-1])
            output.mkdir()
            (output / inventories.OUTPUTS[0]).write_bytes((self.generated / inventories.OUTPUTS[0]).read_bytes())

        with (
            mock.patch.object(inventories, "ROOT", self.root),
            mock.patch.object(inventories.subprocess, "run", side_effect=generate),
            redirect_stdout(io.StringIO()) as output,
        ):
            self.assertEqual(inventories.main(), 1)
        self.assertIn("omitted required inventory: live-feed-compatibility.json", output.getvalue())

    def test_generator_failure_is_not_retried_or_swallowed(self):
        with mock.patch.object(inventories.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "cargo")) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                inventories.check(self.root)
        run.assert_called_once()

    def test_existing_prebuild_entrypoint_stops_before_ui_generation_on_inventory_failure(self):
        with (
            mock.patch.object(inventories, "main", return_value=1) as check,
            mock.patch.object(check_generated_ui_sources.subprocess, "run") as generate_ui,
        ):
            self.assertEqual(check_generated_ui_sources.main(), 1)
        check.assert_called_once()
        generate_ui.assert_not_called()


if __name__ == "__main__":
    unittest.main()
