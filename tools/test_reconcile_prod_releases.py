#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import reconcile_prod_releases as controller  # noqa: E402
import release_reconciler as releases  # noqa: E402


class LiveFeedAllocationTests(unittest.TestCase):
    def test_controller_creates_daemon_owned_release_namespace_parents(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            live_root, scratch_root, state_root = (
                controller.prepare_release_live_feed_paths(root, "2026-08-22.1")
            )

            self.assertEqual(
                live_root, root / "live-feeds/releases/2026-08-22.1"
            )
            self.assertEqual(
                scratch_root, root / "scratch/live-feeds/releases/2026-08-22.1"
            )
            self.assertEqual(
                state_root, root / "state/live-feeds/releases/2026-08-22.1"
            )
            self.assertTrue(live_root.is_dir())
            self.assertTrue(scratch_root.is_dir())
            self.assertTrue(state_root.is_dir())

    def test_daemon_failure_reports_the_latest_specific_error(self) -> None:
        journal = SimpleNamespace(
            stdout=(
                "systemd: service failed\n"
                "daemon: Error: first validation failure\n"
                "daemon: Error: controlling validation failure\n"
                "systemd: restart scheduled\n"
            )
        )
        with mock.patch.object(
            controller.subprocess, "run", return_value=journal
        ):
            detail = controller.service_failure_detail("example.service")

        self.assertEqual(
            detail, "daemon: Error: controlling validation failure"
        )

    def test_each_release_gets_a_stable_distinct_loopback_port(self) -> None:
        observed = releases.ObservedState.empty()
        observed.releases["old"] = releases.ObservedRelease(
            tag="old",
            tag_object="a" * 40,
            commit="b" * 40,
            live_feed_endpoint="http://127.0.0.1:8100",
        )
        self.assertEqual(
            controller.allocate_live_feed_endpoint(observed, port_base=8100),
            "http://127.0.0.1:8101",
        )

    def test_qualification_is_bound_to_current_release_and_product_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release_root = root / "release"
            (release_root / "web").mkdir(parents=True)
            (release_root / "web/index.html").write_text("web", encoding="utf-8")
            (release_root / "downloads").mkdir()
            (release_root / "downloads/app.apk").write_bytes(b"apk")
            (release_root / "bin").mkdir()
            (release_root / "bin/aerobag-live-feedsd").write_bytes(b"live")
            (release_root / "bin/preprocessor-cli").write_bytes(b"preproc")
            release_json = release_root / "release.json"
            release_json.write_text(
                json.dumps(
                    {
                        "tag": "candidate",
                        "commit": "b" * 40,
                        "artifacts": {
                            "web": {
                                "sha256": controller.release_builder.directory_sha256(
                                    release_root / "web"
                                )
                            },
                            "downloads": {
                                "sha256": controller.release_builder.directory_sha256(
                                    release_root / "downloads"
                                )
                            },
                            "apk": {
                                "filename": "app.apk",
                                "sha256": controller._sha256(
                                    release_root / "downloads/app.apk"
                                ),
                            },
                            "live_feeds_binary": {
                                "filename": "aerobag-live-feedsd",
                                "sha256": controller._sha256(
                                    release_root / "bin/aerobag-live-feedsd"
                                ),
                            },
                            "preprocessor_binary": {
                                "filename": "preprocessor-cli",
                                "sha256": controller._sha256(
                                    release_root / "bin/preprocessor-cli"
                                ),
                            },
                        },
                    }
                ),
                encoding="utf-8",
            )
            product = root / "product_artifacts.json"
            product.write_text("product", encoding="utf-8")
            qualification = release_root / "qualification.json"
            qualification.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "tag": "candidate",
                        "commit": "b" * 40,
                        "release_json_sha256": controller._sha256(release_json),
                        "product_manifest_sha256": controller._sha256(product),
                    }
                ),
                encoding="utf-8",
            )
            record = releases.ObservedRelease(
                tag="candidate",
                tag_object="a" * 40,
                commit="b" * 40,
                qualification_status="passed",
                qualification_record=str(qualification),
                release_root=str(release_root),
                product_manifest=str(product),
            )

            self.assertTrue(controller.qualification_is_current(record))
            product.write_text("changed product", encoding="utf-8")
            self.assertFalse(controller.qualification_is_current(record))


class ControllerRecoveryTests(unittest.TestCase):
    def controller(
        self,
        root: Path,
        observed: releases.ObservedState,
    ) -> controller.Controller:
        instance = controller.Controller.__new__(controller.Controller)
        instance.args = SimpleNamespace(
            observed=root / "observed.json",
            controller_preprocessor=root / "controller/preprocessor-cli",
        )
        instance.artifact_root = root
        instance.desired = releases.DesiredReleases(
            production=releases.ReleaseBinding("prod"),
            staging=None,
            sunset=(),
        )
        instance.observed = observed
        return instance

    def test_restart_records_a_generation_activated_before_state_write(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            generation = root / "channel-generations/00000002"
            generation.mkdir(parents=True)
            (generation / "generation.json").write_text(
                '{"generation":2,"production":"prod","staging":null,"sunset":[]}',
                encoding="utf-8",
            )
            (root / "channel-current").symlink_to(
                generation.relative_to(root), target_is_directory=True
            )
            observed = releases.ObservedState(
                releases={
                    "prod": releases.ObservedRelease(
                        tag="prod", tag_object="a" * 40, commit="b" * 40
                    )
                },
                production="prod",
                generation=1,
                channel_inputs_dirty=True,
            )
            instance = self.controller(root, observed)

            with mock.patch.object(controller, "_run") as run:
                self.assertTrue(instance.recover_activated_generation())

            self.assertEqual(instance.observed.generation, 2)
            self.assertFalse(instance.observed.channel_inputs_dirty)
            self.assertTrue(instance.observed.gc_pending)
            self.assertEqual(
                [call.args[0] for call in run.call_args_list],
                [["nginx", "-t"], ["systemctl", "reload", "nginx.service"]],
            )
            persisted = releases.load_observed_state(root / "observed.json")
            self.assertEqual(persisted.generation, 2)

    def test_pending_gc_is_retried_and_cleared_only_after_success(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            observed = releases.ObservedState(
                releases={
                    "prod": releases.ObservedRelease(
                        tag="prod",
                        tag_object="a" * 40,
                        commit="b" * 40,
                        release_root=str(root / "release"),
                    )
                },
                production="prod",
                gc_pending=True,
            )
            instance = self.controller(root, observed)
            with mock.patch.object(controller, "_run") as run:
                instance.run_pending_gc()

            run.assert_called_once_with(
                [
                    str(root / "controller/preprocessor-cli"),
                    "gc",
                    "--build-root",
                    str(root),
                    "--execute",
                ]
            )
            self.assertFalse(instance.observed.gc_pending)

    def test_expired_legacy_singleton_drain_stops_old_service(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            observed = releases.ObservedState(
                legacy_live_feed_draining_until_utc=(
                    datetime.now(timezone.utc) - timedelta(minutes=1)
                ).isoformat().replace("+00:00", "Z")
            )
            instance = self.controller(root, observed)

            with mock.patch.object(controller, "_run") as run:
                instance.stop_completed_drains()

            run.assert_called_once_with(
                [
                    "systemctl",
                    "disable",
                    "--now",
                    "aerobag-live-feeds.service",
                ]
            )
            self.assertIsNone(
                instance.observed.legacy_live_feed_draining_until_utc
            )


if __name__ == "__main__":
    unittest.main()
