#!/usr/bin/env python3

import json
import tempfile
import unittest
from pathlib import Path

import watch_build_log


class WatchBuildLogTests(unittest.TestCase):
    def test_diagnostics_resolve_through_packaged_artifact_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            packaged = root / "published_packaged"
            packaged.mkdir()
            diagnostics_name = "build_errors_20260517.json"
            (packaged / diagnostics_name).write_text(
                json.dumps({"error_count": 0}), encoding="utf-8"
            )
            current_path = root / "current_artifacts.json"
            current_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "artifact_roots": {"packaged": "published_packaged/"},
                        "diagnostics": {
                            "filename": diagnostics_name,
                            "error_count": 0,
                        },
                    }
                ),
                encoding="utf-8",
            )
            state = watch_build_log.BuildState()
            state.final_details = f"current_artifacts={current_path}"

            diagnostics = watch_build_log.read_diagnostics_state(state)

            self.assertEqual(diagnostics.status, "ok")
            self.assertEqual(
                diagnostics.text,
                f"diagnostics: OK count=0 source={diagnostics_name}",
            )

    def test_latest_current_artifacts_uses_publication_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            packaged = root / "published_packaged"
            packaged.mkdir()
            current_path = root / "current_artifacts.json"
            current_path.write_text("{}", encoding="utf-8")

            self.assertEqual(
                watch_build_log.latest_current_artifacts_path(packaged), current_path
            )

    def test_diagnostics_reject_parent_traversal(self) -> None:
        root = Path("/tmp/publication")
        path = watch_build_log.diagnostics_manifest_path(
            root / "current_artifacts.json",
            {"artifact_roots": {"packaged": "published_packaged/"}},
            "../build_errors_20260517.json",
        )

        self.assertIsNone(path)


if __name__ == "__main__":
    unittest.main()
