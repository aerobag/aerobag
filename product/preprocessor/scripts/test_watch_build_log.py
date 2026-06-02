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
            publish_dir = root / "published" / "master" / "20260602T000000Z"
            packaged = publish_dir / "packaged"
            packaged.mkdir(parents=True)
            diagnostics_name = "build_errors_20260517.json"
            (packaged / diagnostics_name).write_text(
                json.dumps({"error_count": 0}), encoding="utf-8"
            )
            current_path = root / "published" / "current_artifacts.json"
            current_path.write_text(
                json.dumps(
                    [
                        {
                            "schema_version": 1,
                            "artifact_roots": {
                                "packaged": "master/20260602T000000Z/packaged/"
                            },
                            "diagnostics": {
                                "filename": diagnostics_name,
                                "error_count": 0,
                            },
                        }
                    ]
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
            (root / "published").mkdir()
            current_path = root / "published" / "current_artifacts.json"
            current_path.write_text("{}", encoding="utf-8")

            self.assertEqual(
                watch_build_log.latest_current_artifacts_path(root), current_path
            )

    def test_product_artifacts_path_uses_publish_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            publish_dir = root / "published" / "master" / "20260602T000000Z"
            publish_dir.mkdir(parents=True)
            product_path = publish_dir / "product_artifacts.json"
            product_path.write_text("{}", encoding="utf-8")

            self.assertEqual(
                watch_build_log.product_artifacts_path(publish_dir), product_path
            )

    def test_parse_product_artifacts_path_from_log_detail(self) -> None:
        self.assertEqual(
            watch_build_log.parse_current_artifacts_path(
                "PASS product_artifacts=/tmp/product_artifacts.json"
            ),
            Path("/tmp/product_artifacts.json"),
        )

    def test_publish_label_is_parsed_from_begin_line(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "+0:00 begin pid=123 profile=production build_root=/tmp/build "
            "publish_dir=/tmp/build/published/nav6-sunset/20260602T000000Z "
            "publish_label=nav6-sunset scheduler=product_weighted_dag"
        )

        self.assertEqual(state.publish_label, "nav6-sunset")

    def test_publish_label_falls_back_to_publish_dir(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "+0:00 begin pid=123 profile=production "
            "build_root=/tmp/pub publish_dir=/tmp/pub/published/nav6-sunset-c641d0f2/20260602T000000Z "
            "scheduler=product_weighted_dag"
        )

        self.assertEqual(state.publish_label, "nav6-sunset-c641d0f2")

    def test_diagnostics_reject_parent_traversal(self) -> None:
        root = Path("/tmp/publication")
        path = watch_build_log.diagnostics_manifest_path(
            root / "current_artifacts.json",
            {"artifact_roots": {"packaged": "master/20260602T000000Z/packaged/"}},
            "../build_errors_20260517.json",
        )

        self.assertIsNone(path)


if __name__ == "__main__":
    unittest.main()
