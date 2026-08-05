#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
import tempfile
import unittest
from datetime import datetime
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
            "+0:00 begin pid=123 build_root=/tmp/build "
            "publish_dir=/tmp/build/published/nav6-sunset/20260602T000000Z "
            "publish_label=nav6-sunset scheduler=product_weighted_dag"
        )

        self.assertEqual(state.publish_label, "nav6-sunset")

    def test_publish_label_falls_back_to_publish_dir(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "+0:00 begin pid=123 "
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

    def test_json_snapshot_reports_progress_and_liveness(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "2026-06-09T21:00:00+00:00 +0:00 begin pid=1 "
            "build_root=/tmp/build publish_dir=/tmp/build/published/master/20260609T210000Z "
            "publish_label=master scheduler=product_weighted_dag"
        )
        state.apply_line(
            "2026-06-09T21:00:01+00:00 +0:01 scheduler-ready tasks=2 work_unit_budget=4"
        )
        state.apply_line(
            "2026-06-09T21:00:02+00:00 +0:02 task event=start id=nav-db "
            "source=product-scheduler launched=1 total=2 completed=0 weight=1 "
            "running_units=1 work_unit_budget=4"
        )
        state.apply_line(
            "2026-06-09T21:00:03+00:00 +0:03 task event=complete id=nav-db "
            "source=product-scheduler status=PASS completed=1 total=2 "
            "running_units=0 work_unit_budget=4 -- published"
        )

        snapshot = watch_build_log.state_snapshot(state, Path("/tmp/build.log"))

        self.assertEqual(snapshot["schema_version"], 1)
        self.assertEqual(snapshot["build"]["publish_label"], "master")
        self.assertEqual(snapshot["progress"]["total_tasks"], 2)
        self.assertEqual(snapshot["progress"]["completed"], 1)
        self.assertEqual(snapshot["progress"]["pending"], 1)
        self.assertEqual(snapshot["tasks"]["completed"][0]["task"], "nav-db")
        self.assertEqual(
            snapshot["tasks"]["completed"][0]["source"], "product-scheduler"
        )
        self.assertEqual(snapshot["process"]["alive"], True)

    def test_completed_task_runtime_uses_completion_time_not_now(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "2026-06-09T21:00:00+00:00 +0:00 begin pid=1 "
            "build_root=/tmp/build publish_dir=/tmp/build/published/master/20260609T210000Z"
        )
        state.apply_line(
            "2026-06-09T21:00:02+00:00 +0:02 task event=start id=nav-db "
            "source=product-scheduler launched=1 total=1 completed=0 weight=1 "
            "running_units=1 work_unit_budget=4"
        )
        state.apply_line(
            "2026-06-09T21:00:05+00:00 +0:05 task event=complete id=nav-db "
            "source=product-scheduler status=PASS completed=1 total=1 "
            "running_units=0 work_unit_budget=4 -- published"
        )

        snapshot = watch_build_log.state_snapshot(
            state,
            Path("/tmp/build.log"),
            now_wall=datetime.fromisoformat("2026-06-09T22:00:00+00:00"),
        )

        completed = snapshot["tasks"]["completed"][0]
        self.assertEqual(completed["runtime_seconds"], 3)
        self.assertEqual(completed["runtime"], "0:03")

    def test_any_named_task_is_visible_without_changing_scheduler_counts(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "2026-08-04T01:00:00+00:00 +10:00 product-scheduler-ready "
            "tasks=154 work_unit_budget=152"
        )
        state.apply_line(
            "2026-08-04T01:00:01+00:00 +10:01 task event=start "
            "id=publication-integrity source=finalization artifacts=126 bytes=11497139397"
        )

        self.assertEqual(state.total_tasks, 154)
        self.assertEqual(len(state.active_tasks()), 1)
        self.assertEqual(state.active_tasks()[0].task, "publication-integrity")
        self.assertEqual(state.active_tasks()[0].source, "finalization")
        self.assertEqual(
            state.active_tasks()[0].details,
            "artifacts=126 bytes=11497139397",
        )

        state.apply_line(
            "2026-08-04T01:00:03+00:00 +10:03 task event=progress "
            "id=publication-integrity source=finalization hashed_files=2 "
            "hashed_bytes=4096 reused_checks=5"
        )
        self.assertIn("hashed_files=2", state.active_tasks()[0].details)

        state.apply_line(
            "2026-08-04T01:00:05+00:00 +10:05 task event=complete "
            "id=publication-integrity source=finalization status=PASS hashed_files=2 "
            "hashed_bytes=4096 reused_checks=250"
        )
        self.assertEqual(state.active_tasks(), [])
        completed = state.recent_completed(1)[0]
        self.assertEqual(completed.task, "publication-integrity")
        self.assertIn("reused_checks=250", completed.details)
        self.assertEqual(state.total_tasks, 154)

    def test_failed_generic_task_becomes_terminal(self) -> None:
        state = watch_build_log.BuildState()
        state.apply_line(
            "2026-08-04T01:00:01+00:00 +1:00 task event=start id=future-task "
            "source=future-subsystem weight=3"
        )
        state.apply_line(
            "2026-08-04T01:00:02+00:00 +1:01 task event=complete id=future-task "
            "source=future-subsystem status=FAIL -- error=deliberate failure"
        )

        self.assertEqual(state.active_tasks(), [])
        completed = state.recent_completed(1)[0]
        self.assertEqual(completed.task, "future-task")
        self.assertEqual(completed.source, "future-subsystem")
        self.assertEqual(completed.status, "failed")
        self.assertEqual(completed.details, "status=FAIL error=deliberate failure")

    def test_incremental_snapshot_reads_only_appended_log_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            log_path = Path(tmp) / "master.log"
            log_path.write_text(
                "\n".join(
                    [
                        "2026-06-09T21:00:00+00:00 +0:00 begin pid=1 "
                        "build_root=/tmp/build publish_dir=/tmp/build/published/master/20260609T210000Z",
                        "2026-06-09T21:00:01+00:00 +0:01 scheduler-ready tasks=3 work_unit_budget=4",
                        "2026-06-09T21:00:02+00:00 +0:02 task event=start id=a "
                        "source=product-scheduler launched=1 total=3 completed=0 weight=1 "
                        "running_units=1 work_unit_budget=4",
                        "2026-06-09T21:00:03+00:00 +0:03 task event=complete id=a "
                        "source=product-scheduler status=PASS completed=1 total=3 "
                        "running_units=0 work_unit_budget=4 -- ok",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            snapshotter = watch_build_log.IncrementalLogSnapshotter(log_path)
            first = snapshotter.snapshot(completed_limit=1)
            self.assertEqual(first["progress"]["completed"], 1)
            self.assertEqual(first["tasks"]["completed"][0]["task"], "a")

            with log_path.open("a", encoding="utf-8") as handle:
                handle.write(
                    "2026-06-09T21:00:04+00:00 +0:04 task event=start id=b "
                    "source=product-scheduler launched=2 total=3 completed=1 weight=1 "
                    "running_units=1 work_unit_budget=4\n"
                )
                handle.write(
                    "2026-06-09T21:00:05+00:00 +0:05 task event=complete id=b "
                    "source=product-scheduler status=PASS completed=2 total=3 "
                    "running_units=0 work_unit_budget=4 -- ok\n"
                )

            second = snapshotter.snapshot(completed_limit=1)
            self.assertEqual(second["progress"]["completed"], 2)
            self.assertEqual(len(second["tasks"]["completed"]), 1)
            self.assertEqual(second["tasks"]["completed"][0]["task"], "b")

    def test_dashboard_uses_cross_platform_font_fallbacks(self) -> None:
        html = watch_build_log.build_dashboard_html(2)

        self.assertIn('--font-sans: "IBM Plex Sans", Inter,', html)
        self.assertIn('"DejaVu Sans", "Liberation Sans", Arial, Helvetica', html)
        self.assertIn('--font-mono: ui-monospace, "Cascadia Mono",', html)
        self.assertIn("font-family: var(--font-sans);", html)
        self.assertIn("font-family: var(--font-mono);", html)


if __name__ == "__main__":
    unittest.main()
