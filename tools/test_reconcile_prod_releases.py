#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import os
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


class ForcedPromotionArgumentTests(unittest.TestCase):
    def test_installed_wrapper_can_supply_force_tag_through_service_environment(
        self,
    ) -> None:
        argv = [
            "reconcile_prod_releases.py",
            "--desired",
            "desired.json",
            "--observed",
            "observed.json",
            "--source-root",
            "source",
            "--artifact-root",
            "artifacts",
            "--cargo-target-dir",
            "target",
            "--controller-preprocessor",
            "preprocessor-cli",
            "--ui-target-root",
            "ui-target",
        ]
        with (
            mock.patch.object(sys, "argv", argv),
            mock.patch.dict(
                os.environ,
                {"AEROBAG_FORCE_PRODUCTION_TAG": "2026-08-23.1"},
            ),
        ):
            args = controller.parse_args()

        self.assertEqual(args.force_production_tag, "2026-08-23.1")


class MaintenancePolicyTests(unittest.TestCase):
    def test_assignment_change_defers_refresh_and_gc_until_periodic_reconcile(self) -> None:
        self.assertEqual(
            controller.maintenance_policy(
                assignment_pending=True,
                refresh_requested=True,
            ),
            (False, False),
        )

    def test_periodic_reconcile_runs_requested_maintenance(self) -> None:
        self.assertEqual(
            controller.maintenance_policy(
                assignment_pending=False,
                refresh_requested=True,
            ),
            (True, True),
        )


class ProgressReportingTests(unittest.TestCase):
    def test_progress_marker_contains_one_atomic_human_scale_sentence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            controller.write_progress(root, "  Building\n release   candidate  ")

            path = root / releases.RECONCILIATION_PROGRESS_RELATIVE_PATH
            self.assertEqual(path.read_text(encoding="utf-8"), "Building release candidate\n")
            self.assertEqual(list(path.parent.glob(".*.tmp")), [])


class PublicProductionValidationTests(unittest.TestCase):
    def test_about_is_required_only_when_the_immutable_release_contains_it(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            current = root / "channel-current/production"
            files = {
                current / "web/index.html": b"index",
                current / "packages/current_artifacts.json": b"packages",
                current / "downloads/android-apk.json": b"apk",
            }
            for path, body in files.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(body)

            instance = controller.Controller.__new__(controller.Controller)
            instance.args = SimpleNamespace(public_origin="https://aerobag.test")
            instance.artifact_root = root
            requested: list[str] = []

            def urlopen(url: str, timeout: int):
                requested.append(url)
                relative = url.removeprefix("https://aerobag.test")
                bodies = {
                    "/": b"index",
                    "/packages/current_artifacts.json": b"packages",
                    "/live-feeds/status.json": b"live",
                    "/downloads/android-apk.json": b"apk",
                    "/about": b"about",
                }
                content_types = {
                    "/": "text/html",
                    "/about": "text/html",
                    "/packages/current_artifacts.json": "application/json",
                    "/live-feeds/status.json": "application/json",
                    "/downloads/android-apk.json": "application/json",
                }
                response = mock.MagicMock()
                response.__enter__.return_value = SimpleNamespace(
                    status=200,
                    read=lambda: bodies[relative],
                    headers=SimpleNamespace(
                        get_content_type=lambda: content_types[relative]
                    ),
                )
                return response

            with mock.patch.object(
                controller.urllib.request,
                "urlopen",
                side_effect=urlopen,
            ):
                instance.validate_public_production()
                self.assertNotIn("https://aerobag.test/about", requested)

                (current / "web/about.html").write_bytes(b"about")
                requested.clear()
                instance.validate_public_production()

            self.assertIn("https://aerobag.test/about", requested)

    def test_staging_qualification_is_exact_http_contract_without_host_chrome(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release_root = root / "release"
            expected = {
                "https://aerobag.test/staging/": (
                    release_root / "web/index.html",
                    b"index",
                    "text/html",
                ),
                "https://aerobag.test/staging/about": (
                    release_root / "web/about.html",
                    b"about",
                    "text/html",
                ),
                "https://aerobag.test/staging/packages/current_artifacts.json": (
                    root
                    / "channel-current/staging/packages/current_artifacts.json",
                    b"packages",
                    "application/json",
                ),
                "https://aerobag.test/staging/live-feeds/status.json": (
                    None,
                    b"live",
                    "application/json",
                ),
                "https://aerobag.test/staging/downloads/android-apk.json": (
                    release_root / "downloads/android-apk.json",
                    b"apk",
                    "application/json",
                ),
            }
            for path, body, _content_type in expected.values():
                if path is not None:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(body)
            (release_root / "release.json").write_text("{}", encoding="utf-8")
            product_manifest = root / "product_artifacts.json"
            product_manifest.write_text("{}", encoding="utf-8")

            record = releases.ObservedRelease(
                tag="candidate",
                tag_object="a" * 40,
                commit="b" * 40,
                build_status="passed",
                release_root=str(release_root),
                product_manifest=str(product_manifest),
            )
            instance = controller.Controller.__new__(controller.Controller)
            instance.args = SimpleNamespace(
                public_origin="https://aerobag.test",
                observed=root / "observed.json",
            )
            instance.artifact_root = root
            instance.observed = releases.ObservedState(
                releases={record.tag: record},
                staging=record.tag,
            )
            content_types = {
                url: expected_content_type
                for url, (_path, _body, expected_content_type) in expected.items()
            }
            about_url = "https://aerobag.test/staging/about"
            content_types[about_url] = "application/octet-stream"

            def urlopen(url: str, timeout: int):
                _path, body, _content_type = expected[url]
                response = mock.MagicMock()
                response.__enter__.return_value = SimpleNamespace(
                    status=200,
                    read=lambda: body,
                    headers=SimpleNamespace(
                        get_content_type=lambda: content_types[url]
                    ),
                )
                return response

            with (
                mock.patch.object(
                    controller.urllib.request,
                    "urlopen",
                    side_effect=urlopen,
                ),
                mock.patch.object(controller, "_run") as run,
                mock.patch.object(
                    controller.release_builder,
                    "validate_release_directory",
                ),
            ):
                with self.assertRaisesRegex(
                    RuntimeError,
                    "unexpected content type",
                ):
                    instance.qualify(record.tag)
                content_types[about_url] = "text/html"
                instance.qualify(record.tag)

            run.assert_not_called()
            self.assertEqual(record.qualification_status, "passed")
            self.assertTrue(Path(record.qualification_record or "").is_file())


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
