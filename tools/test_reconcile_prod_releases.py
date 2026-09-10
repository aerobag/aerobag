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
import threading
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
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


class ForcedPromotionActivationTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        release_root = root / "release"
        for directory in [release_root / "web", release_root / "downloads", root / "published"]:
            directory.mkdir(parents=True)
        self.record = releases.ObservedRelease(
            tag="candidate", tag_object="a" * 40, commit="b" * 40,
            build_status="passed", qualification_status="pending",
            release_root=str(release_root), live_feed_endpoint="http://127.0.0.1:8101",
            live_feed_status="running",
        )
        self.instance = controller.Controller.__new__(controller.Controller)
        self.instance.args = SimpleNamespace(
            observed=root / "observed.json", force_production_tag="candidate",
            controller_preprocessor=root / "preprocessor-cli",
        )
        self.instance.artifact_root = root
        self.instance.desired = releases.DesiredReleases(
            production=releases.ReleaseBinding("candidate"), staging=None, sunset=(),
        )
        self.instance.observed = releases.ObservedState(
            production="old", staging="candidate", generation=1,
            releases={"candidate": self.record},
        )
        previous = root / "channel-generations/00000001"
        releases.materialize_channel_generation(
            previous, root / "published",
            production_manifests=[releases.ChannelManifest(
                release_tag="old", source_path=root / "old-manifest.json",
                document={}, publication_roots=(),
            )],
            staging_manifests=[],
        )
        (root / "channel-current").symlink_to(previous, target_is_directory=True)
        self.instance.save()
        for target, name, options in [
            (self.instance, "_manifest", {"return_value": releases.ChannelManifest(
                release_tag="candidate", source_path=root / "manifest.json",
                document={}, publication_roots=(),
            )}),
            (self.instance, "validate_public_production", {}),
            (controller.release_builder, "normalize_release_permissions", {}),
            (controller.release_builder, "validate_release_directory", {}),
            (controller, "_run", {}),
        ]:
            patcher = mock.patch.object(target, name, **options)
            patcher.start()
            self.addCleanup(patcher.stop)

    def test_forced_activation_records_bypass_and_preserves_it_on_later_reconcile(self) -> None:
        self.instance.activate()
        observed = releases.load_observed_state(self.instance.args.observed)
        self.assertEqual(observed.production, "candidate")
        record = observed.releases["candidate"]
        self.assertEqual(record.qualification_status, "bypassed")
        self.assertEqual(record.qualification_bypass_reason, "forced promotion")
        timestamp = record.qualification_bypassed_at_utc
        self.assertIsNotNone(timestamp)
        self.assertEqual(datetime.fromisoformat(timestamp.replace("Z", "+00:00")).utcoffset(), timedelta(0))
        self.assertIsNone(record.qualification_record)
        self.assertFalse(controller.qualification_is_current(record))
        self.assertEqual(
            releases.plan_reconciliation(self.instance.desired, observed).actions,
            [releases.ReconcileAction("check_deployment", "candidate")],
        )
        self.instance.activate()
        self.assertEqual(self.record.qualification_bypassed_at_utc, timestamp)

    def test_normal_activation_keeps_passing_qualification(self) -> None:
        self.instance.args.force_production_tag = None
        self.record.qualification_status = "passed"
        self.record.deployment_status = "passed"
        self.instance.activate()
        self.assertEqual(self.record.qualification_status, "passed")
        self.assertEqual(self.record.deployment_status, "pending")
        self.assertIsNone(self.record.qualification_bypassed_at_utc)
        self.assertIsNone(self.record.qualification_bypass_reason)

    def test_failed_activation_does_not_record_a_bypass(self) -> None:
        self.instance.validate_public_production.side_effect = RuntimeError("bad route")
        with self.assertRaisesRegex(RuntimeError, "bad route"):
            self.instance.activate()
        persisted = releases.load_observed_state(self.instance.args.observed)
        self.assertEqual(persisted.production, "old")
        self.assertEqual(self.record.qualification_status, "pending")
        self.assertIsNone(self.record.qualification_bypassed_at_utc)
        self.assertEqual(
            (self.instance.artifact_root / "channel-current").resolve().name,
            "00000001",
        )

    def test_restart_recovers_bypass_after_activation_before_state_write(self) -> None:
        with mock.patch.object(self.instance, "save", side_effect=RuntimeError("restart")):
            with self.assertRaisesRegex(RuntimeError, "restart"):
                self.instance.activate()
        self.instance.observed = releases.load_observed_state(self.instance.args.observed)
        self.instance.args.force_production_tag = None
        self.assertTrue(self.instance.recover_activated_generation())
        record = releases.load_observed_state(self.instance.args.observed).releases["candidate"]
        self.assertEqual(record.qualification_status, "bypassed")
        self.assertEqual(record.qualification_bypass_reason, "forced promotion")


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
                qualification_status="bypassed",
                qualification_bypassed_at_utc="2026-09-07T15:00:00Z",
                qualification_bypass_reason="forced promotion",
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
                failed = releases.load_observed_state(instance.args.observed).releases[record.tag]
                self.assertEqual(failed.qualification_status, "failed")
                self.assertEqual(failed.deployment_status, "failed")
                self.assertIn("unexpected content type", failed.deployment_error)
                content_types[about_url] = "text/html"
                instance.qualify(record.tag)

            run.assert_not_called()
            self.assertEqual(record.qualification_status, "passed")
            self.assertEqual(record.deployment_status, "passed")
            self.assertNotEqual(record.deployment_record, record.qualification_record)
            self.assertIsNone(record.qualification_bypassed_at_utc)
            self.assertIsNone(record.qualification_bypass_reason)
            self.assertTrue(Path(record.qualification_record or "").is_file())


class DeploymentLifecycleTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.instance = controller.Controller.__new__(controller.Controller)
        self.instance.args = SimpleNamespace(
            public_origin="https://aerobag.test", observed=self.root / "observed.json",
            force_production_tag=None,
        )
        self.instance.artifact_root = self.root
        self.instance.desired = releases.DesiredReleases(
            production=releases.ReleaseBinding("prod"), staging=None,
            sunset=(releases.SunsetBinding("old", "2099-01-01T00:00:00Z"),),
        )
        self.instance.observed = releases.ObservedState(
            production="prod", sunset=["old"], generation=1,
        )
        self.routes = {}
        self.requested = []
        for tag, channel, base, web_base in [
            ("prod", "production", "", ""),
            ("old", "releases/old", "/releases/old", "/releases/old/web"),
        ]:
            release_root = self.root / "release-builds" / tag
            files = {
                "web/index.html": f"{tag} index".encode(),
                "web/about.html": f"{tag} about".encode(),
                "downloads/android-apk.json": f'{{"tag":"{tag}"}}'.encode(),
                "downloads/app.apk": b"apk",
                "bin/aerobag-live-feedsd": b"live binary",
                "bin/preprocessor-cli": b"producer binary",
            }
            for name, body in files.items():
                path = release_root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(body)
            artifacts = {
                directory: {"sha256": controller.release_builder.directory_sha256(release_root / directory)}
                for directory in ["web", "downloads"]
            }
            for key, name in [
                ("apk", "downloads/app.apk"),
                ("live_feeds_binary", "bin/aerobag-live-feedsd"),
                ("preprocessor_binary", "bin/preprocessor-cli"),
            ]:
                artifacts[key] = {"filename": Path(name).name, "sha256": controller._sha256(release_root / name)}
            (release_root / "release.json").write_text(json.dumps({
                "tag": tag, "commit": "b" * 40, "artifacts": artifacts,
            }), encoding="utf-8")
            product = self.root / f"{tag}-product.json"
            product.write_text(f'{{"tag":"{tag}"}}', encoding="utf-8")
            discovery = self.root / "channel-current" / channel / "packages/current_artifacts.json"
            discovery.parent.mkdir(parents=True)
            discovery.write_text(f'[{{"discovery":"{tag}"}}]', encoding="utf-8")
            record = releases.ObservedRelease(
                tag=tag, tag_object="a" * 40, commit="b" * 40,
                build_status="passed", qualification_status="passed",
                release_root=str(release_root), product_manifest=str(product),
                live_feed_endpoint="http://127.0.0.1:8100", live_feed_status="running",
            )
            qualification = release_root / "qualification.json"
            qualification.write_text(json.dumps({
                "schema_version": 1, "tag": tag, "commit": record.commit,
                "release_json_sha256": controller._sha256(release_root / "release.json"),
                "product_manifest_sha256": controller._sha256(product),
            }), encoding="utf-8")
            record.qualification_record = str(qualification)
            self.instance.observed.releases[tag] = record
            self.routes.update({
                web_base + "/": (200, "text/html", files["web/index.html"]),
                web_base + "/about": (200, "text/html", files["web/about.html"]),
                base + "/packages/current_artifacts.json": (200, "application/json", discovery.read_bytes()),
                base + "/live-feeds/status.json": (200, "application/json", b'{"status":"running"}'),
                base + "/downloads/android-apk.json": (200, "application/json", files["downloads/android-apk.json"]),
            })
        self.instance.save()

    def urlopen(self, url: str, timeout: int):
        self.assertEqual(timeout, 30)
        self.requested.append(url)
        status, content_type, body = self.routes[url.removeprefix(self.instance.args.public_origin)]
        response = mock.MagicMock()
        response.__enter__.return_value = SimpleNamespace(
            status=status, read=lambda: body,
            headers=SimpleNamespace(get_content_type=lambda: content_type),
        )
        return response

    def current(self, tag: str) -> bool:
        return controller.deployment_is_current(
            self.instance.observed.releases[tag], self.instance.observed,
            self.root, self.instance.args.public_origin,
        )

    def test_existing_production_and_sunset_reconcile_to_current_receipts(self) -> None:
        originals = {
            tag: Path(record.qualification_record).read_bytes()
            for tag, record in self.instance.observed.releases.items()
        }
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen):
            self.assertEqual(self.instance.reconcile(plan_only=False), 0)
            self.assertEqual(len(self.requested), 10)
            self.assertEqual(self.instance.reconcile(plan_only=False), 0)
            self.assertEqual(len(self.requested), 10, "converged checks must be a no-op")
        persisted = releases.load_observed_state(self.instance.args.observed)
        for tag, record in persisted.releases.items():
            self.assertEqual(record.deployment_status, "passed")
            self.assertTrue(self.current(tag))
            self.assertEqual(Path(record.qualification_record).read_bytes(), originals[tag])
            receipt = json.loads(Path(record.deployment_record).read_text())
            self.assertEqual(len(receipt["public_response_sha256"]), 5)
            self.assertEqual(receipt["channel_role"], "production" if tag == "prod" else "sunset")

    def test_real_http_endpoints_are_checked_without_build_or_browser(self) -> None:
        routes = self.routes
        requested = []

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                requested.append(self.path)
                status, content_type, body = routes.get(self.path, (404, "text/plain", b"missing"))
                self.send_response(status)
                self.send_header("Content-Type", content_type)
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        worker = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
        worker.start()
        try:
            self.instance.args.public_origin = f"http://127.0.0.1:{server.server_port}"
            with mock.patch.object(controller, "_run") as run:
                self.instance.check_active_deployments()
            run.assert_not_called()
            self.assertEqual(set(requested), set(routes))
            self.assertTrue(self.current("prod"))
            self.assertTrue(self.current("old"))
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=2)

    def test_product_refresh_rechecks_both_channels_without_granting_qualification(self) -> None:
        original_receipts = {
            tag: Path(record.qualification_record).read_bytes()
            for tag, record in self.instance.observed.releases.items()
        }
        refreshed = {}
        for tag in ["prod", "old"]:
            refreshed[tag] = self.root / f"{tag}-refreshed.json"
            refreshed[tag].write_text(f'{{"tag":"{tag}","cycle":"next"}}')
        with (
            mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen),
            mock.patch.object(self.instance, "build_product_manifest", side_effect=lambda tag, force: refreshed[tag]),
        ):
            self.instance.reconcile(plan_only=False)
            self.instance.refresh_products()
            self.assertTrue(self.instance.observed.channel_inputs_dirty)
            self.assertEqual(
                releases.plan_reconciliation(self.instance.desired, self.instance.observed).actions,
                [releases.ReconcileAction("activate_generation")],
            )
            # Activation owns the actual pointer/merged-view change; its tests
            # cover nginx rollback. Finish that boundary, then run the real planner.
            self.instance.observed.channel_inputs_dirty = False
            self.instance.observed.generation += 1
            self.instance.invalidate_deployment_checks()
            self.instance.reconcile(plan_only=False)
        for tag, record in self.instance.observed.releases.items():
            self.assertTrue(self.current(tag))
            self.assertEqual(record.qualification_status, "pending")
            self.assertIsNone(record.qualification_record)
            self.assertEqual(
                (Path(record.release_root) / "qualification.json").read_bytes(), original_receipts[tag],
            )

    def test_failures_are_persisted_and_can_be_repaired_on_next_reconcile(self) -> None:
        original = self.routes["/releases/old/web/about"]
        for response, error in [
            ((200, "text/html", b"wrong app loading shell"), "unexpected bytes"),
            ((200, "application/octet-stream", original[2]), "unexpected content type"),
            ((503, "text/html", b"unavailable"), "failed for"),
            ((200, "text/html", b""), "failed for"),
        ]:
            with self.subTest(response=response), mock.patch.object(
                controller.urllib.request, "urlopen", side_effect=self.urlopen,
            ):
                self.instance.invalidate_deployment_checks()
                self.routes["/releases/old/web/about"] = response
                with self.assertRaisesRegex(RuntimeError, error):
                    self.instance.reconcile(plan_only=False)
                state = releases.load_observed_state(self.instance.args.observed)
                self.assertEqual(state.releases["prod"].deployment_status, "passed")
                self.assertEqual(state.releases["old"].deployment_status, "failed")
                self.assertIn(error, state.releases["old"].deployment_error)
                self.routes["/releases/old/web/about"] = original
                self.instance.reconcile(plan_only=False)
                self.assertIsNone(self.instance.observed.releases["old"].deployment_error)
                self.assertTrue(self.current("old"))

    def test_timeout_records_failure_without_overwriting_last_passing_receipt(self) -> None:
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen):
            self.instance.check_deployment("prod")
        record = self.instance.observed.releases["prod"]
        receipt = Path(record.deployment_record).read_bytes()
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=TimeoutError("timed out")):
            with self.assertRaisesRegex(TimeoutError, "timed out"):
                self.instance.check_deployment("prod")
        self.assertEqual(record.deployment_status, "failed")
        self.assertFalse(self.current("prod"))
        self.assertEqual(Path(record.deployment_record).read_bytes(), receipt)

    def test_deployment_pass_preserves_forced_admission_audit(self) -> None:
        record = self.instance.observed.releases["prod"]
        record.qualification_status = "bypassed"
        record.qualification_record = None
        record.qualification_bypass_reason = "forced promotion"
        record.qualification_bypassed_at_utc = "2026-09-08T19:00:00Z"
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen):
            self.instance.reconcile(plan_only=False)
        self.assertTrue(self.current("prod"))
        self.assertEqual(record.qualification_status, "bypassed")
        self.assertEqual(record.qualification_bypass_reason, "forced promotion")
        self.assertEqual(record.qualification_bypassed_at_utc, "2026-09-08T19:00:00Z")

    def test_receipt_is_invalidated_by_product_discovery_origin_role_or_release_changes(self) -> None:
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen):
            self.instance.reconcile(plan_only=False)
        record = self.instance.observed.releases["prod"]
        paths = [
            Path(record.product_manifest),
            self.root / "channel-current/production/packages/current_artifacts.json",
            Path(record.release_root) / "web/index.html",
            Path(record.deployment_record),
        ]
        for path in paths:
            with self.subTest(path=path):
                original = path.read_bytes()
                path.write_bytes(b"changed")
                self.assertFalse(self.current("prod"))
                path.write_bytes(original)
                self.assertTrue(self.current("prod"))
        self.instance.args.public_origin = "https://changed.test"
        self.assertFalse(self.current("prod"))
        self.instance.args.public_origin = "https://aerobag.test"
        self.instance.observed.production = "other"
        self.instance.observed.sunset.append("prod")
        self.assertFalse(self.current("prod"))

    def test_artifacts_changing_during_checks_cannot_receive_passing_receipt(self) -> None:
        def changed(url, timeout):
            Path(self.instance.observed.releases["prod"].product_manifest).write_text("changed")
            return self.urlopen(url, timeout)

        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=changed):
            with self.assertRaisesRegex(RuntimeError, "artifacts changed during checks"):
                self.instance.check_deployment("prod")
        self.assertEqual(self.instance.observed.releases["prod"].deployment_status, "failed")

    def test_legacy_sunset_without_about_can_pass_without_weakening_staging(self) -> None:
        record = self.instance.observed.releases["old"]
        root = Path(record.release_root)
        (root / "web/about.html").unlink()
        release_json = root / "release.json"
        document = json.loads(release_json.read_text())
        document["artifacts"]["web"]["sha256"] = controller.release_builder.directory_sha256(root / "web")
        release_json.write_text(json.dumps(document))
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen):
            self.instance.check_deployment("old")
        self.assertTrue(self.current("old"))
        self.assertNotIn("https://aerobag.test/releases/old/web/about", self.requested)
        staging = controller.public_channel_checks(
            self.instance.args.public_origin, "staging", "staging", root, root,
        )
        self.assertIn("about", staging)

    def test_inactive_release_cannot_be_checked_or_qualified_as_staging(self) -> None:
        with mock.patch.object(controller.urllib.request, "urlopen") as fetch:
            with self.assertRaisesRegex(RuntimeError, "active on staging"):
                self.instance.qualify("old")
            self.instance.observed.sunset = []
            with self.assertRaisesRegex(RuntimeError, "not active on a channel"):
                self.instance.check_deployment("old")
            with self.assertRaisesRegex(RuntimeError, "must converge"):
                self.instance.check_active_deployments()
        fetch.assert_not_called()

    def test_restart_distrusts_missing_or_stale_deployment_receipts(self) -> None:
        with mock.patch.object(controller.urllib.request, "urlopen", side_effect=self.urlopen):
            self.instance.reconcile(plan_only=False)
        args = SimpleNamespace(
            **vars(self.instance.args), source_root=self.root,
            artifact_root=self.root, desired=self.root / "desired.json",
        )
        resolved = {
            tag: releases.ResolvedTag(tag=tag, tag_object=record.tag_object, commit=record.commit)
            for tag, record in self.instance.observed.releases.items()
        }
        prod = self.instance.observed.releases["prod"]
        path = Path(prod.deployment_record)
        original = path.read_bytes()
        for corrupt in [None, b'{}', b'[]', b'{"schema_version": 99}']:
            with self.subTest(corrupt=corrupt):
                if corrupt is None:
                    path.unlink()
                else:
                    path.write_bytes(corrupt)
                with (
                    mock.patch.object(controller, "_git", return_value="c" * 40),
                    mock.patch.object(releases, "load_desired_releases", return_value=self.instance.desired),
                    mock.patch.object(releases, "resolve_desired_tags", return_value=resolved),
                ):
                    restarted = controller.Controller(args)
                self.assertEqual(restarted.observed.releases["prod"].deployment_status, "pending")
                self.assertEqual(restarted.observed.releases["old"].deployment_status, "passed")
                self.assertEqual(
                    releases.plan_reconciliation(restarted.desired, restarted.observed).actions,
                    [releases.ReconcileAction("check_deployment", "prod")],
                )
                path.write_bytes(original)

    def test_check_only_entrypoint_never_enters_maintenance_or_reconciliation(self) -> None:
        args = SimpleNamespace(artifact_root=self.root, check_deployments_only=True)
        with (
            mock.patch.object(controller, "parse_args", return_value=args),
            mock.patch.object(controller, "Controller", return_value=self.instance),
            mock.patch.object(self.instance, "check_active_deployments") as checks,
            mock.patch.object(self.instance, "refresh_products") as refresh,
            mock.patch.object(self.instance, "run_pending_gc") as gc,
            mock.patch.object(self.instance, "reconcile") as reconcile,
            mock.patch.object(self.instance, "stop_completed_drains") as drains,
        ):
            self.assertEqual(controller.main(), 0)
        checks.assert_called_once_with()
        for forbidden in [refresh, gc, reconcile, drains]:
            forbidden.assert_not_called()

    def test_pending_or_failed_checks_do_not_starve_scheduled_product_refresh(self) -> None:
        for status in ["pending", "failed"]:
            args = SimpleNamespace(
                artifact_root=self.root, check_deployments_only=False,
                plan=False, refresh_products=True, force_production_tag=None,
            )
            self.instance.observed.releases["prod"].deployment_status = status
            with (
                self.subTest(status=status),
                mock.patch.object(controller, "parse_args", return_value=args),
                mock.patch.object(controller, "Controller", return_value=self.instance),
                mock.patch.object(self.instance, "refresh_products") as refresh,
                mock.patch.object(self.instance, "run_pending_gc") as gc,
                mock.patch.object(self.instance, "reconcile", return_value=0) as reconcile,
                mock.patch.object(self.instance, "stop_completed_drains"),
                mock.patch.object(self.instance, "recover_activated_generation"),
            ):
                self.assertEqual(controller.main(), 0)
            refresh.assert_called_once_with()
            gc.assert_called_once_with()
            reconcile.assert_called_once_with(plan_only=False)


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
            product.write_text("product", encoding="utf-8")
            (release_root / "web/index.html").write_text("corrupted web", encoding="utf-8")
            self.assertFalse(controller.qualification_is_current(record))
            (release_root / "web/index.html").write_text("web", encoding="utf-8")
            qualification.write_text("[]", encoding="utf-8")
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
