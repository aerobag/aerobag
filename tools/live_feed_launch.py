# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Immutable, GC-rooted inputs for release live-feed daemon instances."""

from __future__ import annotations

import json
import hashlib
import os
import re
import shutil
import tempfile
import subprocess
import time
import urllib.request
from pathlib import Path
from typing import Callable

import build_release
import live_feed_compatibility as compatibility
import release_reconciler as releases


def instance_root(artifact_root: Path, instance_id: str) -> Path:
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,159}", instance_id):
        raise ValueError(f"unsafe live-feed instance ID {instance_id!r}")
    return releases.owned_path(artifact_root, Path("live-feed-launches") / instance_id)


def pin_launch_inputs(
    artifact_root: Path, tag: str, instance_id: str, product_manifest: Path,
) -> Path:
    root = instance_root(artifact_root, instance_id)
    pinned = root / "packages/product_artifacts.json"
    manifest = releases.load_channel_manifest(tag, product_manifest)
    payload = product_manifest.read_bytes()
    if root.exists():
        if not pinned.is_file() or pinned.read_bytes() != payload:
            raise RuntimeError(f"immutable live-feed launch input mismatch: {root}")
        return pinned
    root.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=f".{instance_id}.", dir=root.parent))
    try:
        packages = temporary / "packages"
        packages.mkdir()
        (packages / pinned.name).write_bytes(payload)
        (packages / "current_artifacts.json").write_text(
            json.dumps([manifest.document], indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        for publication in manifest.publication_roots:
            target = artifact_root / "published" / publication
            if not target.is_dir():
                raise RuntimeError(f"missing live-feed startup publication: {target}")
            (packages / publication).symlink_to(os.path.relpath(target, packages), target_is_directory=True)
        os.replace(temporary, root)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    return pinned


def launch_gc_path(artifact_root: Path, instance_id: str) -> str:
    root = instance_root(artifact_root, instance_id)
    return (root / "packages/current_artifacts.json").relative_to(artifact_root).as_posix()


class LiveFeedRuntime:
    """Prepare and observe instances; the pure reconciler decides their users."""

    def __init__(
        self, artifact_root: Path, observed: releases.ObservedState, *, port_base: int,
        run: Callable, save: Callable,
        env_root: Path = Path("/etc/aerobag/live-feeds"),
    ):
        if type(port_base) is not int or not 1 <= port_base <= 65535 - 99:
            raise ValueError("live-feed port base must leave 100 valid listener ports")
        self.artifact_root = artifact_root.resolve()
        self.observed = observed
        self.port_base = port_base
        self.run = run
        self.save = save
        self.env_root = env_root.resolve()
        self._requirements: dict[tuple[str, ...], dict] = {}

    def _release_evidence(self, record: releases.ObservedRelease) -> tuple[dict, dict | None]:
        if record.release_root is None:
            raise ValueError("release has no immutable artifact root")
        root = Path(record.release_root)
        metadata = json.loads((root / "release.json").read_text(encoding="utf-8"))
        if (not isinstance(metadata, dict) or type(metadata.get("schema_version")) is not int
                or metadata["schema_version"] != 1 or metadata.get("tag") != record.tag
                or metadata.get("commit") != record.commit):
            raise ValueError("release metadata does not identify the expected build")
        artifacts = metadata.get("artifacts")
        if not isinstance(artifacts, dict):
            raise ValueError("release has no artifact identities")
        binary = artifacts.get("live_feeds_binary")
        if not isinstance(binary, dict) or binary.get("filename") != "aerobag-live-feedsd":
            raise ValueError("invalid live-feed executable artifact")
        compatibility._sha256(binary.get("sha256"), "live-feed executable identity")
        if binary["sha256"] != build_release._sha256(root / "bin" / binary["filename"]):
            raise ValueError("live-feed executable identity mismatch")
        if "live_feed_compatibility" not in artifacts:
            return metadata, None
        artifact = artifacts["live_feed_compatibility"]
        path = root / "live-feed-compatibility.json"
        if not isinstance(artifact, dict) or artifact.get("filename") != path.name:
            raise ValueError("invalid live-feed inventory artifact")
        compatibility._sha256(artifact.get("sha256"), "live-feed inventory identity")
        if artifact["sha256"] != build_release._sha256(path):
            raise ValueError("live-feed inventory artifact identity mismatch")
        inventory = json.loads(path.read_text(encoding="utf-8"))
        compatibility.validate_wire_inventory(inventory)
        return metadata, inventory

    def _validate_requirements(self, value: object) -> dict:
        document = compatibility._object(value, {
            "schema_version", "wire_contracts", "notam_catalog", "startup_publication",
        }, "publication compatibility metadata")
        compatibility._evidence_schema(document)
        compatibility.validate_wire_inventory(document["wire_contracts"])
        compatibility.validate_catalog_identity(document["notam_catalog"])
        compatibility._startup_publication(document["startup_publication"])
        return document

    def publication_requirements(self, record: releases.ObservedRelease) -> dict | None:
        if record.release_root is None or record.product_manifest is None:
            return None
        try:
            release_root = Path(record.release_root)
            metadata, inventory = self._release_evidence(record)
            if inventory is None:
                return None
            manifest = Path(record.product_manifest).resolve(strict=True)
            manifest_sha = build_release._sha256(manifest)
            binary_sha = metadata["artifacts"]["live_feeds_binary"]["sha256"]
            key = (record.tag, record.commit, str(release_root.resolve()), str(manifest), manifest_sha,
                   binary_sha, metadata["artifacts"]["live_feed_compatibility"]["sha256"])
            if key in self._requirements:
                requirements = self._requirements[key]
            else:
                binary = release_root / "bin/aerobag-live-feedsd"
                result = subprocess.run(
                    [str(binary), "--describe-compatibility", str(manifest)],
                    check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120,
                )
                requirements = self._validate_requirements(json.loads(result.stdout))
                if requirements["wire_contracts"] != inventory:
                    raise ValueError("client and daemon live-feed inventories differ")
                source = requirements["startup_publication"]
                if source != {"path": str(manifest), "sha256": manifest_sha}:
                    raise ValueError("live-feed descriptor describes a different publication")
                if build_release._sha256(manifest) != manifest_sha or build_release._sha256(binary) != binary_sha:
                    raise ValueError("live-feed inputs changed during description")
        except (OSError, ValueError, TypeError, subprocess.SubprocessError):
            return None
        sidecar = releases.owned_path(
            self.artifact_root, Path("state/live-feed-requirements") / record.tag / f"{manifest_sha}.json",
        )
        document = {
            "schema_version": 1, "tag": record.tag, "commit": record.commit,
            "binary_sha256": binary_sha, "requirements": requirements,
        }
        try:
            persisted = json.loads(sidecar.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            persisted = None
        if persisted != document:
            releases._write_json_atomic(sidecar, document)
        self._requirements[key] = requirements
        return json.loads(json.dumps(requirements))

    def prepare(self, record: releases.ObservedRelease, requirements: dict) -> releases.LiveFeedInstance:
        self._validate_requirements(requirements)
        if record.product_manifest is None:
            raise ValueError("release has no publication manifest")
        manifest = Path(record.product_manifest).resolve(strict=True)
        if requirements["startup_publication"] != {"path": str(manifest), "sha256": build_release._sha256(manifest)}:
            raise ValueError("launch requirements do not identify the current publication")
        identity = {"release": record.commit, "wire_contracts": requirements["wire_contracts"],
                    "notam_catalog": requirements["notam_catalog"]}
        digest = hashlib.sha256(json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        instance_id = f"{record.tag}-{digest[:16]}"
        existing = self.observed.live_feed_instances.get(instance_id)
        if existing is not None:
            if existing.launch_digest != digest or existing.release_tag != record.tag:
                raise RuntimeError(f"live-feed instance identity collision: {instance_id}")
            if existing.status != "removed":
                if build_release._sha256(Path(existing.manifest)) != existing.manifest_sha256:
                    raise RuntimeError(f"immutable live-feed startup manifest changed: {instance_id}")
                return existing
        pinned = pin_launch_inputs(self.artifact_root, record.tag, instance_id, manifest)
        pinned_sha = build_release._sha256(pinned)
        if pinned_sha != requirements["startup_publication"]["sha256"]:
            raise ValueError("live-feed publication changed while pinning launch inputs")
        used = {record.live_feed_endpoint for record in self.observed.releases.values()}
        used.update(instance.endpoint for instance in self.observed.live_feed_instances.values()
                    if instance.status != "removed")
        endpoint = next((f"http://127.0.0.1:{port}" for port in range(self.port_base, self.port_base + 100)
                         if f"http://127.0.0.1:{port}" not in used), None)
        if endpoint is None:
            raise RuntimeError("no free live-feed instance endpoint")
        roots = [str(self.artifact_root / prefix / instance_id) for prefix in
                 ("live-feeds/instances", "scratch/live-feeds/instances", "state/live-feeds/instances")]
        instance = releases.LiveFeedInstance(
            instance_id=instance_id, release_tag=record.tag, launch_digest=digest,
            endpoint=endpoint, unit=f"aerobag-live-feeds-release@{instance_id}.service",
            manifest=str(pinned), manifest_sha256=pinned_sha,
            roots=roots + [str(instance_root(self.artifact_root, instance_id))],
            gc_paths=[launch_gc_path(self.artifact_root, instance_id)],
        )
        self.observed.live_feed_instances[instance_id] = instance
        return instance

    def observe(self, instance: releases.LiveFeedInstance) -> None:
        instance.status = "unavailable"
        instance.evidence = None
        try:
            record = self.observed.releases[instance.release_tag]
            metadata, inventory = self._release_evidence(record)
            expected = metadata["artifacts"]["live_feeds_binary"]["sha256"]
            if inventory is None:
                return
            evidence = compatibility.validate_runtime_envelope(self._direct_json(instance, "compatibility.json"))
            if (evidence["executable_sha256"] != expected
                    or evidence["release_tag"] != instance.release_tag
                    or evidence["launch_instance_id"] != instance.instance_id
                    or evidence["wire_contracts"] != inventory):
                raise ValueError("runtime identity does not match launched daemon")
            if "notams" in evidence["configured_products"]:
                if evidence["notam_catalog"] is None:
                    raise ValueError("configured NOTAMs have no loaded catalog identity")
                if evidence["startup_publication"] != {"path": instance.manifest, "sha256": instance.manifest_sha256}:
                    raise ValueError("runtime publication does not match launch inputs")
                requirements = self.publication_requirements(releases.ObservedRelease(
                    tag=record.tag, tag_object=record.tag_object, commit=record.commit,
                    release_root=record.release_root, product_manifest=instance.manifest,
                ))
                if (requirements is None or evidence["notam_catalog"] != requirements["notam_catalog"]
                        or evidence["startup_publication"] != requirements["startup_publication"]):
                    raise ValueError("runtime catalog does not match launch inputs")
            # Warming or disabled products affect sharing eligibility, not process health.
            instance.evidence = evidence
            instance.status = "running"
        except (OSError, ValueError, TypeError, AttributeError, KeyError, subprocess.SubprocessError):
            # Missing or invalid modern evidence cannot verify a runtime instance.
            return

    def _direct_json(self, instance: releases.LiveFeedInstance, resource: str) -> object:
        import urllib.error
        from urllib.parse import urlsplit
        class DirectListenerOnly(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, request, response, code, message, headers, new_url):
                raise urllib.error.HTTPError(request.full_url, code, "direct listener redirected", headers, response)
        address = urlsplit(instance.endpoint)
        if (address.scheme != "http" or address.hostname != "127.0.0.1" or address.port is None
                or address.username is not None or address.password is not None
                or address.path or address.query or address.fragment):
            raise ValueError("live-feed process verification requires a direct loopback listener")
        url = f"{instance.endpoint}/live-feeds/{resource}"
        request = urllib.request.Request(url, headers={"Cache-Control": "no-cache, no-store"})
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), DirectListenerOnly())
        with opener.open(request, timeout=2) as response:
            if response.status != 200 or response.geturl() != url:
                raise ValueError("daemon identity probe was redirected or unsuccessful")
            value = json.load(response)
        if not isinstance(value, dict):
            raise ValueError("daemon response is not a JSON object")
        return value

    def _write_launch_environment(self, instance: releases.LiveFeedInstance) -> None:
        record = self.observed.releases[instance.release_tag]
        self._release_evidence(record)
        if (instance.manifest_sha256 is None
                or build_release._sha256(Path(instance.manifest)) != instance.manifest_sha256):
            raise ValueError("live-feed launch inputs are missing or changed; prepare the instance first")
        live_root, scratch_root, state_root = map(Path, instance.roots[:3])
        for root in (live_root, scratch_root, state_root):
            root.mkdir(parents=True, exist_ok=True)
        self.env_root.mkdir(parents=True, exist_ok=True)
        environment = releases.owned_path(self.env_root, f"{instance.instance_id}.env")
        values = {
            "AEROBAG_RELEASE_ROOT": record.release_root,
            "AEROBAG_RELEASE_TAG": record.tag,
            "AEROBAG_RELEASE_LIVE_INSTANCE_ID": instance.instance_id,
            "AEROBAG_RELEASE_LIVE_LISTEN": instance.endpoint.removeprefix("http://"),
            "AEROBAG_RELEASE_LIVE_ROOT": str(live_root),
            "AEROBAG_RELEASE_LIVE_SCRATCH": str(scratch_root),
            releases.RELEASE_LIVE_FEEDS_STATE_ENV: str(state_root),
            "AEROBAG_RELEASE_FETCH_CACHE": str(self.artifact_root / "cache/fetch"),
            "AEROBAG_RELEASE_PRODUCT_ARTIFACTS": instance.manifest,
        }
        contents = "".join(f"{key}={json.dumps(value)}\n" for key, value in values.items())
        if environment.exists():
            if environment.read_text() != contents:
                raise RuntimeError(f"immutable live-feed launch environment changed: {environment}")
            return
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=self.env_root, delete=False) as stream:
                temporary = Path(stream.name)
                stream.write(contents)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, environment)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)

    def start(self, instance: releases.LiveFeedInstance) -> None:
        was_removed = instance.status == "removed"
        instance.status = "pending"
        instance.evidence = None
        self.save()
        service_requested = False
        try:
            if was_removed:
                raise ValueError("prepare removed live-feed instances before starting them")
            self._write_launch_environment(instance)
            self.run(["systemctl", "daemon-reload"])
            service_requested = True
            self.run(["systemctl", "enable", "--now", instance.unit])
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                self.observe(instance)
                if instance.status == "running":
                    instance.draining_until_utc = None
                    self.save()
                    return
                time.sleep(1)
            raise RuntimeError(f"live-feed daemon process could not be verified: {instance.unit}")
        except Exception as error:
            instance.status = "failed"
            instance.evidence = None
            if service_requested:
                try:
                    self.run(["systemctl", "disable", "--now", instance.unit])
                except Exception as cleanup_error:
                    error.add_note(f"failed to disable {instance.unit}: {cleanup_error}")
            self.save()
            raise
