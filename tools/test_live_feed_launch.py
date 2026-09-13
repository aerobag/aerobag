# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import copy
import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import live_feed_launch as launch
import live_feed_compatibility as compatibility
import reconcile_prod_releases as controller
from test_live_feed_compatibility import requirement, provider


class LaunchSnapshotTests(unittest.TestCase):
    def test_snapshot_survives_original_manifest_deletion_and_roots_its_packages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            publication = root / "published/publication-a"
            (publication / "unpacked").mkdir(parents=True)
            (publication / "packaged").mkdir()
            source = publication / "product_artifacts.json"
            document = {
                "schema_version": 1,
                "contracts": {"nav-db": "NAV25"},
                "artifact_roots": {
                    "packaged": "publication-a/packaged",
                    "unpacked": "publication-a/unpacked",
                },
                "bundles": [],
            }
            source.write_text(json.dumps(document))
            pinned = launch.pin_launch_inputs(root, "test", "test-0123456789ab", source)
            source.unlink()
            self.assertEqual(json.loads(pinned.read_text()), document)
            self.assertTrue((pinned.parent / "publication-a/unpacked").is_dir())
            self.assertEqual(json.loads((pinned.parent / "current_artifacts.json").read_text()), [document])

    def test_existing_launch_input_cannot_be_rewritten_or_escape_owned_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "published/a/packaged").mkdir(parents=True)
            (root / "published/a/unpacked").mkdir()
            source = root / "product_artifacts.json"
            document = {"schema_version": 1, "contracts": {"nav-db": "NAV25"},
                        "artifact_roots": {"packaged": "a/packaged", "unpacked": "a/unpacked"}, "bundles": []}
            source.write_text(json.dumps(document))
            first = launch.pin_launch_inputs(root, "test", "test-0123456789ab", source)
            self.assertEqual(launch.pin_launch_inputs(root, "test", "test-0123456789ab", source), first)
            document["as_of_date"] = "2026-09-14"
            source.write_text(json.dumps(document))
            with self.assertRaisesRegex(RuntimeError, "immutable"):
                launch.pin_launch_inputs(root, "test", "test-0123456789ab", source)
            with self.assertRaises(ValueError):
                launch.pin_launch_inputs(root, "test", "../../outside", source)


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.release_root = self.root / "releases/test"
        (self.release_root / "bin").mkdir(parents=True)
        self.binary = self.release_root / "bin/aerobag-live-feedsd"
        self.binary.write_bytes(b"test executable")
        self.inventory = self.release_root / "live-feed-compatibility.json"
        self.inventory.write_text(json.dumps(compatibility.WIRE_INVENTORY))
        publication = self.root / "published/publication-a"
        (publication / "unpacked").mkdir(parents=True)
        (publication / "packaged").mkdir()
        self.manifest = publication / "product_artifacts.json"
        self.manifest.write_text(json.dumps({
            "schema_version": 1, "contracts": {"nav-db": "NAV25"},
            "artifact_roots": {"packaged": "publication-a/packaged", "unpacked": "publication-a/unpacked"},
            "bundles": [], "as_of_utc": "2026-09-13T00:00:00Z",
        }))
        self.record = launch.releases.ObservedRelease(
            tag="test", tag_object="a" * 40, commit="b" * 40,
            release_root=str(self.release_root), product_manifest=str(self.manifest),
        )
        self.metadata = {"schema_version": 1, "tag": self.record.tag, "commit": self.record.commit, "artifacts": {
            "live_feeds_binary": {"filename": self.binary.name, "sha256": launch.build_release._sha256(self.binary)},
            "live_feed_compatibility": {"filename": self.inventory.name, "sha256": launch.build_release._sha256(self.inventory)},
        }}
        self.write_metadata()
        self.observed = launch.releases.ObservedState(releases={self.record.tag: self.record})
        self.run = mock.Mock()
        self.save = mock.Mock()
        self.runtime = launch.LiveFeedRuntime(self.root, self.observed, port_base=19220,
            run=self.run, save=self.save, env_root=self.root / "environment")
        self.requirements = requirement()
        self.requirements["startup_publication"] = {"path": str(self.manifest), "sha256": launch.build_release._sha256(self.manifest)}

    def write_metadata(self):
        (self.release_root / "release.json").write_text(json.dumps(self.metadata))

    def describe(self, value=None):
        return mock.patch.object(launch.subprocess, "run", return_value=subprocess.CompletedProcess(
            [str(self.binary)], 0, stdout=json.dumps(self.requirements if value is None else value)))

    def instance(self):
        return self.runtime.prepare(self.record, self.requirements)

    def evidence(self, instance):
        result = provider(self.record.tag)
        result.update(executable_sha256=self.metadata["artifacts"]["live_feeds_binary"]["sha256"],
            launch_instance_id=instance.instance_id,
            startup_publication={"path": instance.manifest, "sha256": instance.manifest_sha256})
        return result

    @contextlib.contextmanager
    def http(self, instance, evidence, *, status=200, redirect=False):
        def response(url, **kwargs):
            target = url.full_url if isinstance(url, launch.urllib.request.Request) else url
            self.assertEqual(target, instance.endpoint + "/live-feeds/compatibility.json")
            stream = io.BytesIO(json.dumps(evidence).encode())
            stream.status = status
            stream.geturl = lambda: "http://127.0.0.1:29999/wrong-provider" if redirect else target
            return stream
        def describe(command, **kwargs):
            self.assertEqual(command[:2], [str(self.binary), "--describe-compatibility"])
            source = Path(command[2])
            output = copy.deepcopy(self.requirements)
            output["startup_publication"] = {"path": str(source), "sha256": launch.build_release._sha256(source)}
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps(output))
        with mock.patch.object(launch.urllib.request, "build_opener", return_value=mock.Mock(open=response)), \
                mock.patch.object(launch.subprocess, "run", side_effect=describe):
            yield

    def test_requirements_match_full_generated_inventory_and_bind_manifest(self):
        with self.describe() as describe:
            result = self.runtime.publication_requirements(self.record)
        self.assertEqual(result, self.requirements)
        self.assertEqual(describe.call_args.args[0], [str(self.binary), "--describe-compatibility", str(self.manifest)])
        sidecar = self.root / "state/live-feed-requirements/test" / f"{self.requirements['startup_publication']['sha256']}.json"
        self.assertEqual(json.loads(sidecar.read_text())["requirements"], self.requirements)

    def test_missing_or_malformed_release_files_are_unknown_not_exceptions(self):
        metadata_path = self.release_root / "release.json"
        for contents in (None, "{", "null", "[]", "{}", '{"artifacts":null}'):
            with self.subTest(contents=contents):
                if contents is None:
                    metadata_path.unlink(missing_ok=True)
                else:
                    metadata_path.write_text(contents)
                with self.describe() as describe:
                    self.assertIsNone(self.runtime.publication_requirements(self.record))
                    describe.assert_not_called()

    def test_unknown_or_incomplete_requirements_never_become_launch_requirements(self):
        for value in ([], {}, {**self.requirements, "schema_version": True},
                      {**self.requirements, "schema_version": 2},
                      {**self.requirements, "notam_catalog": None},
                      {**self.requirements, "unexpected": True}):
            with self.subTest(value=value), self.describe(value):
                self.assertIsNone(self.runtime.publication_requirements(self.record))

    def test_identically_incomplete_inventory_is_not_trusted(self):
        broken = copy.deepcopy(self.requirements)
        broken["wire_contracts"]["products"].pop("notams")
        self.inventory.write_text(json.dumps(broken["wire_contracts"]))
        self.metadata["artifacts"]["live_feed_compatibility"]["sha256"] = launch.build_release._sha256(self.inventory)
        self.write_metadata()
        with self.describe(broken):
            self.assertIsNone(self.runtime.publication_requirements(self.record))

    def test_cached_requirements_reverify_executable_identity(self):
        with self.describe():
            self.assertIsNotNone(self.runtime.publication_requirements(self.record))
        self.binary.write_bytes(b"replaced binary")
        with self.describe() as describe:
            self.assertIsNone(self.runtime.publication_requirements(self.record))
            describe.assert_not_called()

    def test_missing_malformed_or_mismatched_inventory_files_are_unknown(self):
        for contents in (None, "{", "null", "{}"):
            with self.subTest(contents=contents):
                if contents is None:
                    self.inventory.unlink()
                else:
                    self.inventory.write_text(contents)
                    self.metadata["artifacts"]["live_feed_compatibility"]["sha256"] = launch.build_release._sha256(self.inventory)
                    self.write_metadata()
                with self.describe() as describe:
                    self.assertIsNone(self.runtime.publication_requirements(self.record))
                    describe.assert_not_called()

    def test_descriptor_command_failure_is_unknown_and_does_not_cache_a_certificate(self):
        for error in (subprocess.TimeoutExpired("describe", 120), subprocess.CalledProcessError(1, "describe"), OSError("no executable")):
            with self.subTest(error=error), mock.patch.object(launch.subprocess, "run", side_effect=error):
                self.assertIsNone(self.runtime.publication_requirements(self.record))
        with self.describe():
            self.assertEqual(self.runtime.publication_requirements(self.record), self.requirements)

    def serving_controller(self):
        self.record.build_status = self.record.deployment_status = "passed"
        self.observed.production = self.record.tag
        self.observed.generation = 1
        generation = self.root / "channel-generations/00000001"
        generation.mkdir(parents=True)
        (self.root / "channel-current").symlink_to(generation, target_is_directory=True)
        result = controller.Controller.__new__(controller.Controller)
        result.artifact_root = self.root
        result.args = SimpleNamespace(observed=self.root / "observed.json")
        result.desired = launch.releases.DesiredReleases(
            launch.releases.ReleaseBinding(self.record.tag), None, (),
        )
        result.observed = self.observed
        result._live_feed_runtime = self.runtime
        return result

    def assert_description_failure_blocks_existing_instance(self, *, refreshed):
        instance = self.instance()
        instance.status, instance.evidence = "running", self.evidence(instance)
        self.record.live_feed_instance = self.record.live_feed_provider = instance.instance_id
        self.record.live_feed_requirements = copy.deepcopy(self.requirements)
        current = self.serving_controller()
        if refreshed:
            replacement = self.manifest.with_name("replacement-product_artifacts.json")
            document = json.loads(self.manifest.read_text())
            document["as_of_utc"] = "2026-09-14T00:00:00Z"
            replacement.write_text(json.dumps(document))
            self.record.product_manifest = str(replacement)
            self.observed.channel_inputs_dirty = True
        current.save()
        before = copy.deepcopy(self.observed.to_dict())
        persisted = current.args.observed.read_bytes()
        pinned = Path(instance.manifest).read_bytes()
        with (
            mock.patch.object(launch.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "describe")),
            mock.patch.object(self.runtime, "observe") as observe,
            mock.patch.object(self.runtime, "prepare") as prepare,
        ):
            with self.assertRaisesRegex(launch.releases.ReleaseConfigError, "cannot prepare.*test.*publication"):
                current.refresh_live_feed_observations()
        prepare.assert_not_called()
        observe.assert_not_called()
        self.run.assert_not_called()
        self.assertEqual(self.observed.to_dict(), before)
        self.assertEqual(current.args.observed.read_bytes(), persisted)
        self.assertEqual(Path(instance.manifest).read_bytes(), pinned)
        self.assertEqual((self.root / "channel-current").resolve().name, "00000001")

    def test_controller_blocks_refreshed_publication_when_description_fails(self):
        self.assert_description_failure_blocks_existing_instance(refreshed=True)

    def test_controller_blocks_unchanged_modern_publication_when_description_fails(self):
        self.assert_description_failure_blocks_existing_instance(refreshed=False)

    def test_controller_keeps_pre_feature_release_without_inventory_legacy_dedicated(self):
        self.metadata["artifacts"].pop("live_feed_compatibility")
        self.inventory.unlink()
        self.write_metadata()
        self.record.live_feed_endpoint = "http://127.0.0.1:19220"
        self.record.live_feed_status = "running"
        current = self.serving_controller()
        with self.describe() as describe, mock.patch.object(self.runtime, "prepare") as prepare:
            current.refresh_live_feed_observations()
        describe.assert_not_called()
        prepare.assert_not_called()
        self.assertIsNone(self.record.live_feed_instance)
        self.assertIsNone(self.record.live_feed_requirements)
        binding = launch.releases.resolve_live_feed_bindings(current.desired, self.observed)[self.record.tag]
        self.assertFalse(binding.shared)
        self.assertEqual(binding.endpoint, self.record.live_feed_endpoint)
        self.assertTrue(launch.releases.plan_reconciliation(current.desired, self.observed).converged)

    def test_same_commit_under_new_tag_rechecks_binary_and_writes_its_own_sidecar(self):
        with self.describe():
            self.runtime.publication_requirements(self.record)
        self.record.tag = "second-tag"
        self.metadata["tag"] = self.record.tag
        self.binary.write_bytes(b"different build of same commit")
        self.metadata["artifacts"]["live_feeds_binary"]["sha256"] = launch.build_release._sha256(self.binary)
        self.write_metadata()
        with self.describe() as describe:
            self.assertIsNotNone(self.runtime.publication_requirements(self.record))
            describe.assert_called_once()
        source_sha = self.requirements["startup_publication"]["sha256"]
        for tag in ("test", "second-tag"):
            stored = json.loads((self.root / f"state/live-feed-requirements/{tag}/{source_sha}.json").read_text())
            self.assertEqual(stored["tag"], tag)
        self.assertNotEqual(
            json.loads((self.root / f"state/live-feed-requirements/test/{source_sha}.json").read_text())["binary_sha256"],
            self.metadata["artifacts"]["live_feeds_binary"]["sha256"],
        )

    def test_mutating_returned_requirements_does_not_mutate_cached_evidence(self):
        with self.describe():
            first = self.runtime.publication_requirements(self.record)
            first["notam_catalog"]["sha256"] = "f" * 64
            self.assertEqual(self.runtime.publication_requirements(self.record), self.requirements)

    def test_cached_requirements_restore_retired_or_corrupt_sidecars(self):
        with self.describe():
            self.runtime.publication_requirements(self.record)
        sidecar = self.root / "state/live-feed-requirements/test" / f"{self.requirements['startup_publication']['sha256']}.json"
        for content in (None, "{invalid"):
            with self.subTest(content=content):
                if content is None:
                    launch.shutil.rmtree(sidecar.parent)
                else:
                    sidecar.write_text(content)
                with self.describe() as command:
                    self.runtime.publication_requirements(self.record)
                    command.assert_not_called()
                self.assertEqual(json.loads(sidecar.read_text())["requirements"], self.requirements)

    def test_requirements_cache_does_not_confuse_identical_manifests_at_different_paths(self):
        with self.describe():
            self.runtime.publication_requirements(self.record)
        other = self.manifest.parent / "another-manifest.json"
        other.write_bytes(self.manifest.read_bytes())
        self.record.product_manifest = str(other)
        expected = copy.deepcopy(self.requirements)
        expected["startup_publication"]["path"] = str(other)
        with self.describe(expected):
            self.assertEqual(self.runtime.publication_requirements(self.record), expected)

    def test_prepare_catalog_identity_ignores_publication_timestamp(self):
        first = self.instance()
        document = json.loads(self.manifest.read_text())
        document["as_of_utc"] = "2026-09-14T00:00:00Z"
        self.manifest.write_text(json.dumps(document))
        self.requirements["startup_publication"]["sha256"] = launch.build_release._sha256(self.manifest)
        self.assertIs(self.instance(), first)
        self.assertNotEqual(first.manifest_sha256, self.requirements["startup_publication"]["sha256"])

    def test_prepare_removed_instance_recreates_inputs_and_clears_stale_evidence(self):
        first = self.instance()
        first.status = "removed"
        first.evidence = self.evidence(first)
        first.draining_until_utc = "2026-09-14T00:00:00Z"
        launch.shutil.rmtree(launch.instance_root(self.root, first.instance_id))
        prepared = self.instance()
        self.assertEqual(prepared.instance_id, first.instance_id)
        self.assertEqual(prepared.status, "pending")
        self.assertIsNone(prepared.evidence)
        self.assertIsNone(prepared.draining_until_utc)
        self.assertTrue(Path(prepared.manifest).is_file())

    def test_removed_instance_recreation_can_use_new_timestamp_but_retains_catalog_id(self):
        first = self.instance()
        old_sha = first.manifest_sha256
        first.status = "removed"
        launch.shutil.rmtree(launch.instance_root(self.root, first.instance_id))
        self.manifest.write_text(self.manifest.read_text() + "\n")
        self.requirements["startup_publication"]["sha256"] = launch.build_release._sha256(self.manifest)
        recreated = self.instance()
        self.assertEqual(recreated.instance_id, first.instance_id)
        self.assertNotEqual(recreated.manifest_sha256, old_sha)

    def test_ports_come_from_runtime_context_and_skip_retained_endpoints(self):
        base = self.runtime.port_base
        self.record.live_feed_endpoint = f"http://127.0.0.1:{base}"
        retained = launch.releases.LiveFeedInstance("retained", "test", "digest", f"http://127.0.0.1:{base + 1}", "unit", "manifest")
        self.observed.live_feed_instances[retained.instance_id] = retained
        self.assertEqual(self.instance().endpoint, f"http://127.0.0.1:{base + 2}")

    def test_removed_instances_do_not_exhaust_ports_but_stopped_instances_reserve_them(self):
        base = self.runtime.port_base
        for offset in range(100):
            instance = launch.releases.LiveFeedInstance(f"removed-{offset}", "test", "digest",
                f"http://127.0.0.1:{base + offset}", "unit", "manifest", status="removed")
            self.observed.live_feed_instances[instance.instance_id] = instance
        self.observed.live_feed_instances["removed-0"].status = "stopped"
        self.assertEqual(self.instance().endpoint, f"http://127.0.0.1:{base + 1}")

    def test_removed_instance_recreation_reallocates_its_reassigned_port(self):
        first = self.instance()
        first.status = "removed"
        launch.shutil.rmtree(launch.instance_root(self.root, first.instance_id))
        self.observed.live_feed_instances["retained"] = launch.releases.LiveFeedInstance(
            "retained", "test", "digest", first.endpoint, "unit", "manifest", status="stopped")
        recreated = self.instance()
        self.assertEqual(recreated.instance_id, first.instance_id)
        self.assertEqual(recreated.endpoint, f"http://127.0.0.1:{self.runtime.port_base + 1}")

    def test_prepare_rejects_stale_requirements_before_pinning(self):
        self.manifest.write_text(self.manifest.read_text() + "\n")
        with self.assertRaisesRegex(ValueError, "current publication"):
            self.instance()
        self.assertFalse((self.root / "live-feed-launches").exists())

    def test_prepare_rejects_manifest_change_during_pinning(self):
        pin = launch.pin_launch_inputs
        def change_manifest(*args):
            self.manifest.write_text(self.manifest.read_text() + "\n")
            return pin(*args)
        with mock.patch.object(launch, "pin_launch_inputs", side_effect=change_manifest):
            with self.assertRaisesRegex(ValueError, "changed while pinning"):
                self.instance()
        self.assertEqual(self.observed.live_feed_instances, {})

    def test_relative_artifact_roots_emit_absolute_launch_paths(self):
        runtime = launch.LiveFeedRuntime(Path(os.path.relpath(self.root)), self.observed,
            port_base=self.runtime.port_base, run=self.run, save=self.save, env_root=self.root / "env")
        instance = runtime.prepare(self.record, self.requirements)
        self.assertTrue(Path(instance.manifest).is_absolute())
        self.assertTrue(all(Path(path).is_absolute() for path in instance.roots))

    def test_invalid_port_ranges_are_rejected_before_preparing_any_inputs(self):
        for port in (0, -1, True, 65437, 65536):
            with self.subTest(port=port), self.assertRaises(ValueError):
                launch.LiveFeedRuntime(self.root, self.observed, port_base=port, run=self.run, save=self.save)

    def test_observe_valid_ready_daemon_accepts_exact_launch_identity(self):
        instance = self.instance()
        evidence = self.evidence(instance)
        with self.http(instance, evidence), mock.patch.object(compatibility, "validate_runtime_envelope",
                wraps=compatibility.validate_runtime_envelope) as validate:
            self.runtime.observe(instance)
        validate.assert_called_once_with(evidence)
        self.assertEqual(instance.status, "running")
        self.assertEqual(instance.evidence, evidence)

    def test_observe_requires_instance_and_configured_notam_identity_beyond_nullable_envelope(self):
        instance = self.instance()
        for key in ("release_tag", "launch_instance_id", "notam_catalog", "startup_publication"):
            evidence = self.evidence(instance)
            evidence.update(ready=False, projection_ready=False, published_state_id=None)
            evidence[key] = None
            with self.subTest(key=key):
                self.assertEqual(compatibility.validate_runtime_envelope(evidence), evidence)
                instance.status, instance.evidence = "running", self.evidence(instance)
                with self.http(instance, evidence):
                    self.runtime.observe(instance)
                self.assertEqual(instance.status, "unavailable")
                self.assertIsNone(instance.evidence)

    def test_observe_rejects_invalid_identity_and_malformed_not_unready_evidence(self):
        instance = self.instance()
        valid = self.evidence(instance)
        for key, value in (("ready", "true"), ("ready", 1), ("projection_ready", 0),
                           ("executable_sha256", "f" * 64), ("process_instance_id", ""),
                           ("process_instance_id", None), ("release_tag", "another-tag"),
                           ("schema_version", 99), ("schema_version", True),
                           ("configured_products", "notams"), ("configured_products", ["notams", "notams"]),
                           ("configured_products", ["unknown"]), ("configured_products", [{}]),
                           ("launch_instance_id", "wrong-instance"), ("published_state_id", 1),
                           ("notam_catalog", None), ("startup_publication", None),
                           ("startup_publication", {"path": str(self.manifest), "sha256": instance.manifest_sha256}),
                           ("startup_publication", {"path": instance.manifest, "sha256": "f" * 64}),
                           ("notam_catalog", {**valid["notam_catalog"], "schema_version": True}),
                           ("notam_catalog", {**valid["notam_catalog"], "sha256": "f" * 64})):
            with self.subTest(key=key, value=value):
                instance.status, instance.evidence = "running", valid
                evidence = {**valid, key: value}
                with self.http(instance, evidence):
                    self.runtime.observe(instance)
                self.assertNotEqual(instance.status, "running")
                self.assertIsNone(instance.evidence)

    def test_observe_requires_full_build_inventory_even_for_disabled_products(self):
        instance = self.instance()
        for changed in (False, True):
            evidence = self.evidence(instance)
            evidence.update(configured_products=[], notam_catalog=None, startup_publication=None,
                projection_ready=False, published_state_id=None, ready=False)
            if changed:
                evidence["wire_contracts"]["products"]["notams"]["formats"]["records"]["schema_version"] += 1
            else:
                del evidence["wire_contracts"]["products"]["notams"]
            with self.subTest(changed=changed), self.http(instance, evidence):
                self.runtime.observe(instance)
            self.assertEqual(instance.status, "unavailable")
            self.assertIsNone(instance.evidence)

    def test_warming_notams_still_require_verified_loaded_catalog_and_source(self):
        instance = self.instance()
        evidence = self.evidence(instance)
        evidence.update(ready=False, projection_ready=False, published_state_id=None)
        evidence["notam_catalog"]["sha256"] = "f" * 64
        with self.http(instance, evidence):
            self.runtime.observe(instance)
        self.assertEqual(instance.status, "unavailable")
        self.assertIsNone(instance.evidence)

    def test_observe_malformed_evidence_does_not_retain_a_previous_certificate(self):
        instance = self.instance()
        valid = self.evidence(instance)
        for evidence in (None, [], "not-a-descriptor", {**valid, "unknown": True}):
            with self.subTest(evidence=evidence), self.http(instance, evidence):
                instance.status, instance.evidence = "running", valid
                self.runtime.observe(instance)
                self.assertEqual(instance.status, "unavailable")
                self.assertIsNone(instance.evidence)

    def test_observe_rejects_redirected_or_non_200_certificate(self):
        instance = self.instance()
        for kwargs in ({"redirect": True}, {"status": 503}):
            with self.subTest(kwargs=kwargs), self.http(instance, self.evidence(instance), **kwargs):
                self.runtime.observe(instance)
                self.assertNotEqual(instance.status, "running")
                self.assertIsNone(instance.evidence)

    def test_start_waits_only_for_verified_process_evidence(self):
        instance = self.instance()
        evidence = self.evidence(instance)
        evidence.update(ready=False, projection_ready=False, published_state_id=None)
        observations = [None, evidence]
        calls = []
        real_observe = self.runtime.observe
        def observed(current):
            calls.append(current.instance_id)
            with self.http(current, observations.pop(0)):
                real_observe(current)
        with mock.patch.object(self.runtime, "observe", side_effect=observed), mock.patch.object(launch.time, "sleep"):
            self.runtime.start(instance)
        self.assertEqual(len(calls), 2)
        self.assertEqual(instance.status, "running")
        self.assertEqual(instance.evidence, evidence)

    def assert_dedicated_without_restarts(self, instance, evidence):
        with self.http(instance, evidence), mock.patch.object(launch.time, "sleep") as sleep, \
                mock.patch.object(compatibility, "compare_live_feed_compatibility",
                                  side_effect=AssertionError("sharing comparator used as process-health gate")):
            self.runtime.start(instance)
            sleep.assert_not_called()
        self.assertEqual(instance.status, "running")
        self.assertEqual(instance.evidence, evidence)
        self.assertFalse(compatibility.compare_live_feed_compatibility(self.requirements, evidence).compatible)
        self.record.live_feed_instance = instance.instance_id
        self.record.live_feed_requirements = self.requirements
        self.record.build_status = self.record.deployment_status = self.record.qualification_status = "passed"
        old = copy.deepcopy(self.record)
        old.tag = "old"
        own = copy.deepcopy(instance)
        own.instance_id = "old-instance"
        own.release_tag = old.tag
        own.endpoint = f"http://127.0.0.1:{self.runtime.port_base + 1}"
        own.evidence = provider(old.tag)
        old.live_feed_instance = own.instance_id
        self.observed.releases[old.tag] = old
        self.observed.live_feed_instances[own.instance_id] = own
        self.observed.production = self.record.tag
        self.observed.sunset = [old.tag]
        self.observed.generation = 1
        desired = launch.releases.DesiredReleases(launch.releases.ReleaseBinding(self.record.tag), None,
            (launch.releases.SunsetBinding(old.tag, "2026-10-01T00:00:00Z",
                                          launch.releases.LiveFeedsPolicy.SHARE_IF_COMPATIBLE),))
        bindings = launch.releases.resolve_live_feed_bindings(desired, self.observed)
        self.assertFalse(bindings[old.tag].shared)
        self.assertEqual(bindings[old.tag].provider, own.instance_id)
        launch.releases.validate_live_feed_bindings(desired, self.observed, bindings)
        for tag, binding in bindings.items():
            self.observed.releases[tag].live_feed_provider = binding.provider
            self.observed.releases[tag].live_feed_reason = binding.reason
        for _ in range(3):
            with self.http(instance, evidence):
                self.runtime.observe(instance)
            self.assertEqual(instance.status, "running")
            self.assertEqual(instance.evidence, evidence)
            self.assertTrue(launch.releases.plan_reconciliation(desired, self.observed).converged)
        self.assertEqual(self.run.call_args_list, [mock.call(["systemctl", "daemon-reload"]),
            mock.call(["systemctl", "enable", "--now", instance.unit])])

    def test_cold_warming_process_starts_and_runs_dedicated_without_restart_loop(self):
        instance = self.instance()
        evidence = self.evidence(instance)
        evidence.update(ready=False, projection_ready=False, published_state_id=None)
        self.assert_dedicated_without_restarts(instance, evidence)

    def test_nms_disabled_process_starts_and_runs_dedicated_without_restart_loop(self):
        instance = self.instance()
        evidence = self.evidence(instance)
        evidence["configured_products"].remove("notams")
        evidence.update(ready=False, projection_ready=False, published_state_id=None,
            notam_catalog=None, startup_publication=None)
        with mock.patch.object(self.runtime, "publication_requirements",
                               side_effect=AssertionError("disabled NOTAMs requested a catalog")):
            self.assert_dedicated_without_restarts(instance, evidence)

    def test_partial_product_availability_runs_dedicated_without_restart_loop(self):
        instance = self.instance()
        evidence = self.evidence(instance)
        evidence["configured_products"] = ["notams"]
        self.assert_dedicated_without_restarts(instance, evidence)

    def test_start_timeout_marks_failed_and_disables_only_its_unit(self):
        instance = self.instance()
        with self.http(instance, None), \
                mock.patch.object(launch.time, "monotonic", side_effect=[0, 0, 61]), \
                mock.patch.object(launch.time, "sleep"):
            with self.assertRaisesRegex(RuntimeError, "process could not be verified"):
                self.runtime.start(instance)
        self.assertEqual(instance.status, "failed")
        self.assertIsNone(instance.evidence)
        self.run.assert_any_call(["systemctl", "disable", "--now", instance.unit])

    def test_start_service_failure_clears_stale_success_and_persists_failed_status(self):
        instance = self.instance()
        instance.status, instance.evidence = "running", self.evidence(instance)
        self.run.side_effect = [None, RuntimeError("service failed"), None]
        with self.assertRaisesRegex(RuntimeError, "service failed"):
            self.runtime.start(instance)
        self.assertEqual(instance.status, "failed")
        self.assertIsNone(instance.evidence)
        self.save.assert_called()

    def test_start_invalid_inputs_clear_old_evidence_without_touching_a_service(self):
        instance = self.instance()
        instance.status, instance.evidence = "running", self.evidence(instance)
        Path(instance.manifest).write_text("changed manifest")
        with self.assertRaisesRegex(ValueError, "inputs are missing or changed"):
            self.runtime.start(instance)
        self.assertEqual(instance.status, "failed")
        self.assertIsNone(instance.evidence)
        self.run.assert_not_called()

    def test_missing_modern_certificate_is_unverified_without_additional_probes(self):
        instance = self.instance()
        with self.http(instance, None), mock.patch.object(launch.subprocess, "run") as command:
            self.runtime.observe(instance)
            self.assertEqual(instance.status, "unavailable")
            self.assertIsNone(instance.evidence)
            command.assert_not_called()
        self.run.assert_not_called()

    def test_missing_feature_metadata_is_unverified_without_listener_or_process_probes(self):
        instance = self.instance()
        del self.metadata["artifacts"]["live_feed_compatibility"]
        self.write_metadata()
        with mock.patch.object(self.runtime, "_direct_json") as listener, \
                mock.patch.object(launch.subprocess, "run") as command:
            self.runtime.observe(instance)
            self.assertEqual(instance.status, "unavailable")
            self.assertIsNone(instance.evidence)
            listener.assert_not_called()
            command.assert_not_called()
        self.run.assert_not_called()

    @unittest.skipUnless(os.environ.get("AEROBAG_LIVE_FEEDS_DAEMON_BINARY") and os.environ.get("AEROBAG_NAV_DB_ROLLOVER_BINARY"),
                         "requires explicitly built daemon and compact NAVDB generator binaries")
    def test_actual_offline_daemon_descriptor_matches_generated_two_cycle_navdb(self):
        daemon = Path(os.environ["AEROBAG_LIVE_FEEDS_DAEMON_BINARY"]).resolve(strict=True)
        generator = Path(os.environ["AEROBAG_NAV_DB_ROLLOVER_BINARY"]).resolve(strict=True)
        publication = self.root / "published/compact"
        subprocess.run([str(generator), "--output-root", str(publication), "--scenario", "success",
                        "--transition-at", "2026-09-13T00:00:00Z"], check=True, capture_output=True, text=True, timeout=60)
        document, = json.loads((publication / "current_artifacts.json").read_text())
        self.assertEqual(len(document["bundles"]), 2)
        # The client rollover lab emits bundle manifests only in its download tree.
        for bundle in document["bundles"]:
            relative = bundle["relative_path"]
            launch.shutil.copyfile(publication / "packaged" / relative, publication / "unpacked" / relative)
        document["artifact_roots"] = {"packaged": "compact/packaged", "unpacked": "compact/unpacked"}
        self.manifest = publication / "product_artifacts.json"
        self.manifest.write_text(json.dumps(document))
        self.record.product_manifest = str(self.manifest)
        self.binary.unlink()
        self.binary.symlink_to(daemon)
        self.metadata["artifacts"]["live_feeds_binary"]["sha256"] = launch.build_release._sha256(self.binary)
        self.write_metadata()
        result = subprocess.run([str(self.binary), "--describe-compatibility", str(self.manifest)],
            capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        raw = json.loads(result.stdout)
        self.runtime._validate_requirements(raw)
        self.assertEqual(raw["wire_contracts"], compatibility.WIRE_INVENTORY)
        actual = self.runtime.publication_requirements(self.record)
        self.assertIsNotNone(actual, "real executable output was not accepted by Runtime")
        self.assertEqual(actual["wire_contracts"], compatibility.WIRE_INVENTORY)
        source = json.loads((Path(__file__).resolve().parents[1] / "crates/nav-db-fixture/source.json").read_text())
        catalog = source["records"]["airport/notam-catalog"]
        self.assertEqual(actual["notam_catalog"]["schema_version"], catalog["schema_version"])
        self.assertEqual(actual["notam_catalog"]["airport_count"], len(set(catalog["airport_ids"])))
        instance = self.runtime.prepare(self.record, actual)
        pinned_record = launch.releases.ObservedRelease(tag=self.record.tag, tag_object=self.record.tag_object,
            commit=self.record.commit, release_root=self.record.release_root, product_manifest=instance.manifest)
        self.manifest.unlink()
        pinned = self.runtime.publication_requirements(pinned_record)
        self.assertEqual(pinned["wire_contracts"], actual["wire_contracts"])
        self.assertEqual(pinned["notam_catalog"], actual["notam_catalog"])
        self.assertEqual(pinned["startup_publication"], {"path": instance.manifest, "sha256": instance.manifest_sha256})


if __name__ == "__main__":
    unittest.main()
