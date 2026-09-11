#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Converge a production host on deploy/releases.json without in-place rebuilds."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import build_release as release_builder  # noqa: E402
import release_reconciler as releases  # noqa: E402
import release_retirement as retirement  # noqa: E402


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--desired", type=Path, required=True)
    parser.add_argument("--observed", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--cargo-target-dir", type=Path, required=True)
    parser.add_argument("--controller-preprocessor", type=Path, required=True)
    parser.add_argument("--ui-target-root", type=Path, required=True)
    parser.add_argument("--public-origin", default="https://aerobag.org")
    parser.add_argument("--live-port-base", type=int, default=8100)
    parser.add_argument(
        "--legacy-deployed-rev-file",
        type=Path,
        default=Path("/etc/aerobag/deployed-rev"),
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--plan", action="store_true")
    mode.add_argument(
        "--check-deployments-only",
        action="store_true",
        help="check existing channel endpoints without builds, refresh, activation, or GC",
    )
    parser.add_argument("--refresh-products", action="store_true")
    parser.add_argument(
        "--force-production-tag",
        default=os.environ.get("AEROBAG_FORCE_PRODUCTION_TAG"),
        help="allow this exact active staging tag to become production without qualification",
    )
    return parser.parse_args()


def _run(command: list[str], *, cwd: Path | None = None, env=None) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def _git(source_root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=source_root,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def allocate_live_feed_endpoint(
    observed: releases.ObservedState,
    *,
    port_base: int,
) -> str:
    used = {
        int(record.live_feed_endpoint.rsplit(":", 1)[1])
        for record in observed.releases.values()
        if record.live_feed_endpoint is not None
    }
    for port in range(port_base, port_base + 100):
        if port not in used:
            return f"http://127.0.0.1:{port}"
    raise RuntimeError("no free release live-feed port in configured range")


def prepare_release_live_feed_paths(
    artifact_root: Path, tag: str
) -> tuple[Path, Path, Path]:
    live_root = artifact_root / "live-feeds/releases" / tag
    scratch_root = artifact_root / "scratch/live-feeds/releases" / tag
    state_root = artifact_root / "state/live-feeds/releases" / tag
    # The daemon creates the contract tree below live_root, but deliberately
    # requires these controller-owned release roots to exist.
    live_root.mkdir(parents=True, exist_ok=True)
    scratch_root.mkdir(parents=True, exist_ok=True)
    state_root.mkdir(parents=True, exist_ok=True)
    return live_root, scratch_root, state_root


def service_failure_detail(unit: str) -> str | None:
    result = subprocess.run(
        ["journalctl", "-u", unit, "-n", "40", "--no-pager"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    errors = [line for line in lines if "Error:" in line]
    return (errors or lines)[-1] if lines else None


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def qualification_is_current(record: releases.ObservedRelease) -> bool:
    if (
        record.qualification_status != "passed"
        or record.qualification_record is None
        or record.release_root is None
        or record.product_manifest is None
    ):
        return False
    try:
        release_builder.validate_release_directory(
            Path(record.release_root), record.tag, record.commit
        )
        qualification = json.loads(
            Path(record.qualification_record).read_text(encoding="utf-8")
        )
        return (
            isinstance(qualification, dict)
            and qualification.get("schema_version") == 1
            and qualification.get("tag") == record.tag
            and qualification.get("commit") == record.commit
            and qualification.get("release_json_sha256")
            == _sha256(Path(record.release_root) / "release.json")
            and qualification.get("product_manifest_sha256")
            == _sha256(Path(record.product_manifest))
        )
    except (OSError, ValueError, TypeError, RuntimeError):
        return False


def deployment_channel(observed: releases.ObservedState, tag: str) -> tuple[str, str]:
    """Return the active role and channel directory; never probe a guessed URL."""
    if observed.production == tag:
        return "production", "production"
    if observed.staging == tag:
        return "staging", "staging"
    if tag in observed.sunset:
        return "sunset", f"releases/{tag}"
    raise RuntimeError(f"release {tag} is not active on a channel")


def public_channel_checks(
    origin: str, role: str, channel: str, channel_root: Path, release_root: Path
) -> dict[str, tuple[str, Path | None, str]]:
    base = origin.rstrip("/") + ("" if role == "production" else f"/{channel}")
    web_base = base + ("/web" if role == "sunset" else "")
    checks = {
        "web": (f"{web_base}/", release_root / "web/index.html", "text/html"),
        "packages": (
            f"{base}/packages/current_artifacts.json",
            channel_root / "packages/current_artifacts.json",
            "application/json",
        ),
        "live_feeds": (f"{base}/live-feeds/status.json", None, "application/json"),
        "apk": (
            f"{base}/downloads/android-apk.json",
            release_root / "downloads/android-apk.json",
            "application/json",
        ),
    }
    # Older retained releases legitimately predate the standalone About page.
    # New staging candidates still have to supply it.
    about = release_root / "web/about.html"
    if role == "staging" or about.is_file():
        checks["about"] = (f"{web_base}/about", about, "text/html")
    return checks


def check_public_responses(
    checks: dict[str, tuple[str, Path | None, str]], context: str
) -> dict[str, str]:
    responses = {}
    for name, (url, expected_path, expected_content_type) in checks.items():
        expected_digest = _sha256(expected_path) if expected_path is not None else None
        with urllib.request.urlopen(url, timeout=30) as response:
            body = response.read()
            if response.status != 200 or not body:
                raise RuntimeError(f"{context} failed for {url}")
            if response.headers.get_content_type() != expected_content_type:
                raise RuntimeError(
                    f"{context} received unexpected content type from {url}: "
                    f"{response.headers.get_content_type()}"
                )
            digest = hashlib.sha256(body).hexdigest()
            if expected_digest is not None and digest != expected_digest:
                raise RuntimeError(f"{context} received unexpected bytes from {url}")
            responses[name] = digest
    return responses


def deployment_evidence(
    record: releases.ObservedRelease,
    observed: releases.ObservedState,
    artifact_root: Path,
    public_origin: str,
) -> dict:
    if record.release_root is None or record.product_manifest is None:
        raise RuntimeError(f"release {record.tag} is missing deployment artifacts")
    role, channel = deployment_channel(observed, record.tag)
    release_root = Path(record.release_root)
    release_builder.validate_release_directory(release_root, record.tag, record.commit)
    return {
        "schema_version": 1,
        "kind": "deployed-channel-checks",
        "tag": record.tag,
        "commit": record.commit,
        "channel_role": role,
        "public_origin": public_origin.rstrip("/"),
        "release_json_sha256": _sha256(release_root / "release.json"),
        "product_manifest_sha256": _sha256(Path(record.product_manifest)),
        # Production discovery also includes retained releases. A change to
        # that merged view needs checking even when this release is unchanged.
        "channel_manifest_sha256": _sha256(
            artifact_root / "channel-current" / channel / "packages/current_artifacts.json"
        ),
    }


def deployment_is_current(
    record: releases.ObservedRelease,
    observed: releases.ObservedState,
    artifact_root: Path,
    public_origin: str,
) -> bool:
    if record.deployment_status != "passed" or record.deployment_record is None:
        return False
    try:
        receipt = json.loads(Path(record.deployment_record).read_text(encoding="utf-8"))
        expected = deployment_evidence(record, observed, artifact_root, public_origin)
        return isinstance(receipt, dict) and all(
            receipt.get(key) == value for key, value in expected.items()
        )
    except (OSError, ValueError, TypeError, RuntimeError):
        return False


def maintenance_policy(
    *, assignment_pending: bool, refresh_requested: bool
) -> tuple[bool, bool]:
    """Return whether to run GC and product refresh before reconciliation."""

    if assignment_pending:
        # Assignment changes must converge promptly. Periodic reconciliation
        # performs maintenance after the new channel generation is active.
        return False, False
    return True, refresh_requested


def write_progress(artifact_root: Path, message: str) -> None:
    """Atomically expose one human-scale reconciliation status sentence."""

    path = artifact_root / releases.RECONCILIATION_PROGRESS_RELATIVE_PATH
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(" ".join(message.split()) + "\n", encoding="utf-8")
    os.replace(temporary, path)


class Controller:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.source_root = args.source_root.resolve()
        self.artifact_root = args.artifact_root.resolve()
        self.desired = releases.effective_desired_releases(
            releases.load_desired_releases(args.desired)
        )
        if (
            args.force_production_tag is not None
            and args.force_production_tag != self.desired.production.tag
        ):
            raise RuntimeError(
                "forced production tag does not match the desired production release"
            )
        self.resolved = releases.resolve_desired_tags(self.source_root, self.desired)
        self.observed = releases.load_observed_state(args.observed)
        self.observed.desired_commit = _git(self.source_root, "rev-parse", "HEAD")
        for tag, resolved in self.resolved.items():
            existing = self.observed.releases.get(tag)
            if existing is not None:
                releases.verify_release_identity(resolved, existing)
            else:
                self.observed.releases[tag] = releases.ObservedRelease(
                    tag=tag,
                    tag_object=resolved.tag_object,
                    commit=resolved.commit,
                )
        for record in self.observed.releases.values():
            if (
                record.qualification_status == "passed"
                and not qualification_is_current(record)
            ):
                record.qualification_status = "pending"
                record.last_error = "qualification no longer matches release artifacts"
            if record.deployment_status == "passed" and not deployment_is_current(
                record, self.observed, self.artifact_root, self.args.public_origin
            ):
                record.deployment_status = "pending"
                record.deployment_error = (
                    "deployment checks no longer match current channel artifacts"
                )

    def save(self) -> None:
        releases.write_observed_state(self.args.observed, self.observed)

    def progress(self, message: str) -> None:
        write_progress(self.artifact_root, message)

    def stop_completed_drains(
        self, *, protected_tags: set[str] | None = None, now: datetime | None = None,
    ) -> None:
        now = now or datetime.now(timezone.utc)
        changed = False
        if self.observed.legacy_live_feed_draining_until_utc is not None:
            deadline = datetime.fromisoformat(
                self.observed.legacy_live_feed_draining_until_utc.replace(
                    "Z", "+00:00"
                )
            )
            if deadline <= now:
                _run(
                    [
                        "systemctl",
                        "disable",
                        "--now",
                        "aerobag-live-feeds.service",
                    ]
                )
                self.observed.legacy_live_feed_draining_until_utc = None
                changed = True
        active_tags = set(self.desired.tags()) | set(filter(None, [
            self.observed.production, self.observed.staging, *self.observed.sunset,
        ]))
        for tag, record in self.observed.releases.items():
            if record.draining_until_utc is None:
                continue
            if tag in active_tags:
                record.draining_until_utc = None
                changed = True
                continue
            if protected_tags and tag in protected_tags:
                continue
            deadline = datetime.fromisoformat(
                record.draining_until_utc.replace("Z", "+00:00")
            )
            if deadline > now:
                continue
            _run(
                [
                    "systemctl",
                    "disable",
                    "--now",
                    f"aerobag-live-feeds-release@{tag}.service",
                ]
            )
            record.live_feed_status = "stopped"
            record.draining_until_utc = None
            changed = True
        if changed:
            self.save()

    def maintain_retirement(
        self, *, now: datetime | None = None,
        env_root: Path = Path("/etc/aerobag/live-feeds"),
    ) -> None:
        """Finish retirement even without a new deployment or product refresh.

        The caller holds the reconciler lock. Fail closed on unsafe paths,
        missing retained manifests, or a service that cannot be inspected/stopped.
        """
        now = now or datetime.now(timezone.utc)

        def retention_plan():
            return releases.generation_retention(self.artifact_root, now=now, draining_until={
                tag: datetime.fromisoformat(record.draining_until_utc.replace("Z", "+00:00"))
                for tag, record in self.observed.releases.items()
                if record.draining_until_utc is not None
            })

        retention = retention_plan()
        if not retention.retained:
            return  # No authoritative serving generation: never infer garbage.
        protected = set(self.desired.tags()) | set(retention.release_tags) | set(filter(None, [
            self.observed.production, self.observed.staging, *self.observed.sunset,
        ]))
        self.stop_completed_drains(protected_tags=protected, now=now)
        candidates = []
        recovered_drain = False
        for tag, record in self.observed.releases.items():
            if tag in protected:
                continue
            paths = retirement.release_paths(self.artifact_root, record)
            environment = releases.owned_path(env_root, f"{tag}.env")
            if environment.exists() and not environment.is_file():
                raise releases.ReleaseConfigError(f"unexpected release environment: {environment}")
            # Include historical stopped/failed releases, not just today's drains.
            if not any(path.exists() for path in [*paths, environment]) and record.release_root is None and record.live_feed_endpoint is None:
                continue
            unit = f"aerobag-live-feeds-release@{tag}.service"
            result = subprocess.run(
                ["systemctl", "show", unit, "--property=ActiveState", "--value"],
                check=True, text=True, stdout=subprocess.PIPE, timeout=30,
            )
            if result.stdout.strip() not in {"inactive", "failed"}:
                # Recover a missing drain record after interrupted activation;
                # never delete files merely because observed state says stopped.
                if record.draining_until_utc is None:
                    record.draining_until_utc = (now + releases.RELEASE_DRAIN_GRACE).isoformat().replace("+00:00", "Z")
                    record.live_feed_status = "running"
                    self.save()
                    recovered_drain = True
                continue
            if record.draining_until_utc is not None and datetime.fromisoformat(
                record.draining_until_utc.replace("Z", "+00:00")
            ) > now:
                continue
            # A failed/inactive but enabled unit must not restart after cleanup.
            _run(["systemctl", "disable", "--now", unit])
            candidates.append((record, paths, environment))
        if recovered_drain:
            retention = retention_plan()
            candidates = [entry for entry in candidates if entry[0].tag not in retention.release_tags]
        registry = self.artifact_root / releases.RELEASE_GC_ROOTS
        current_roots = json.loads(registry.read_text()).get("current_artifacts_paths") if registry.exists() else None
        if not candidates and not retention.expired and current_roots == list(retention.gc_paths):
            return
        # Persist the retry obligation BEFORE releasing roots or deleting files.
        self.observed.gc_pending = True
        self.save()
        releases.write_generation_gc_roots(self.artifact_root, retention.gc_paths)
        for generation in retention.expired:
            print(f"Removing retired channel generation {generation.name}", flush=True)
            retirement.remove_owned_path(self.artifact_root, generation)
        for record, paths, environment in candidates:
            print(f"Removing retired release files for {record.tag}", flush=True)
            for path in paths:
                retirement.remove_owned_path(self.artifact_root, path)
            retirement.remove_owned_path(env_root, environment)
            retirement.forget_removed_artifacts(record)
            self.save()

    def recover_activated_generation(self) -> bool:
        current_link = self.artifact_root / "channel-current"
        if not current_link.is_symlink():
            return False
        generation = current_link.resolve()
        metadata_path = generation / "generation.json"
        if not metadata_path.is_file():
            return False
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        number = metadata.get("generation")
        if not isinstance(number, int) or number <= self.observed.generation:
            return False
        expected = {
            "production": self.desired.production.tag,
            "staging": self.ready_staging_tag(),
            "sunset": [binding.tag for binding in self.desired.sunset],
        }
        actual = {key: metadata.get(key) for key in expected}
        if actual != expected:
            raise RuntimeError(
                f"active unrecorded generation {generation} does not match desired state"
            )
        _run(["nginx", "-t"])
        _run(["systemctl", "reload", "nginx.service"])
        self.record_qualification_bypass(metadata.get("qualification_bypass"))
        self.observed.production = expected["production"]
        self.observed.staging = expected["staging"]
        self.observed.sunset = expected["sunset"]
        self.observed.generation = number
        self.observed.channel_inputs_dirty = False
        self.observed.gc_pending = True
        self.invalidate_deployment_checks()
        self.save()
        return True

    def run_pending_gc(self) -> None:
        if not self.observed.gc_pending:
            return
        self.progress("Garbage-collecting unreferenced release artifacts")
        _run(
            [
                str(self.controller_preprocessor()),
                "gc",
                "--build-root",
                str(self.artifact_root),
                "--execute",
            ]
        )
        self.observed.gc_pending = False
        self.save()

    def validate_public_production(self) -> None:
        current = self.artifact_root / "channel-current/production"
        check_public_responses(
            public_channel_checks(
                self.args.public_origin, "production", "production", current, current
            ),
            "activated production channel validation",
        )

    def build_product_manifest(self, tag: str, *, force: bool) -> Path:
        record = self.observed.releases[tag]
        result_path = self.artifact_root / "state/release-build-results" / f"{tag}.json"
        if not force and result_path.is_file():
            result = json.loads(result_path.read_text(encoding="utf-8"))
            matches = [
                item
                for item in result.get("releases", [])
                if item.get("ref") == tag and item.get("commit") == record.commit
            ]
            if len(matches) == 1:
                candidate = Path(matches[0]["product_artifacts"])
                if candidate.is_file():
                    return candidate
        _run(
            [
                str(
                    self.source_root
                    / "product/preprocessor/scripts/build_multi_version_publication.py"
                ),
                "--release",
                "--no-activate",
                "--results-output",
                str(result_path),
                "--primary-ref",
                tag,
                "--build-root",
                str(self.artifact_root),
                "--target-dir",
                str(self.args.cargo_target_dir),
                tag,
            ],
            cwd=self.source_root,
        )
        result = json.loads(result_path.read_text(encoding="utf-8"))
        matches = [
            item
            for item in result.get("releases", [])
            if item.get("ref") == tag and item.get("commit") == record.commit
        ]
        if len(matches) != 1:
            raise RuntimeError(f"publication build returned no unique receipt for {tag}")
        manifest = Path(matches[0]["product_artifacts"])
        if not manifest.is_file():
            raise RuntimeError(f"publication build returned missing manifest {manifest}")
        return manifest

    def build(self, tag: str) -> None:
        record = self.observed.releases[tag]
        try:
            self.progress(f"Preparing cycle products for {tag}")
            manifest_path = self.build_product_manifest(tag, force=False)

            self.progress(f"Building client and server artifacts for {tag}")
            worktree = self.artifact_root / "worktrees/releases" / f"{tag}-{record.commit[:12]}"
            if worktree.exists():
                _run(
                    ["git", "worktree", "remove", "--force", str(worktree)],
                    cwd=self.source_root,
                )
            worktree.parent.mkdir(parents=True, exist_ok=True)
            _run(
                ["git", "worktree", "add", "--detach", str(worktree), record.commit],
                cwd=self.source_root,
            )
            try:
                command = [
                    str(self.source_root / "tools/build_release.py"),
                    "--tag",
                    tag,
                    "--commit",
                    record.commit,
                    "--repo-root",
                    str(worktree),
                    "--artifact-root",
                    str(self.artifact_root),
                    "--cargo-target-dir",
                    str(self.args.cargo_target_dir),
                    "--ui-target-root",
                    str(self.args.ui_target_root),
                    "--public-origin",
                    self.args.public_origin,
                ]
                _run(command, cwd=worktree, env=os.environ.copy())
            finally:
                _run(
                    ["git", "worktree", "remove", "--force", str(worktree)],
                    cwd=self.source_root,
                )

            release_root = release_builder.release_directory(
                self.artifact_root, tag, record.commit
            )
            record.product_manifest = str(manifest_path)
            record.release_root = str(release_root)
            record.build_status = "passed"
            record.qualification_status = "pending"
            record.deployment_status = "pending"
            record.deployment_record = None
            record.deployment_error = None
            record.last_error = None
        except BaseException as error:
            record.build_status = "failed"
            record.last_error = str(error)
            self.save()
            raise
        self.save()

    def refresh_products(self) -> None:
        for tag in self.desired.tags():
            record = self.observed.releases[tag]
            if record.build_status != "passed":
                continue
            self.progress(f"Refreshing cycle products for {tag}")
            try:
                manifest = self.build_product_manifest(tag, force=True)
            except BaseException as error:
                record.last_error = f"product refresh failed: {error}"
                self.save()
                continue
            if record.product_manifest != str(manifest):
                record.product_manifest = str(manifest)
                self.observed.channel_inputs_dirty = True
                # Candidate-backed qualification is tied to the exact product
                # manifest. A refreshed cycle must be exercised again before
                # this release can later be promoted or used for rollback.
                record.qualification_status = "pending"
                record.qualification_record = None
                record.deployment_status = "pending"
                record.deployment_error = None
            record.last_error = None
            self.save()

    def start_live_feeds(self, tag: str) -> None:
        self.progress(f"Starting live feeds for {tag}")
        record = self.observed.releases[tag]
        if record.live_feed_endpoint is None:
            record.live_feed_endpoint = allocate_live_feed_endpoint(
                self.observed, port_base=self.args.live_port_base
            )
            self.save()
        port = record.live_feed_endpoint.rsplit(":", 1)[1]
        environment_root = Path("/etc/aerobag/live-feeds")
        environment_root.mkdir(parents=True, exist_ok=True)
        environment = environment_root / f"{tag}.env"
        release_root = Path(record.release_root or "")
        if record.product_manifest is None:
            raise RuntimeError(f"release {tag} has no product manifest")
        live_root, scratch_root, state_root = prepare_release_live_feed_paths(
            self.artifact_root, tag
        )
        values = {
            "AEROBAG_RELEASE_ROOT": str(release_root),
            "AEROBAG_RELEASE_TAG": tag,
            "AEROBAG_RELEASE_LIVE_LISTEN": f"127.0.0.1:{port}",
            "AEROBAG_RELEASE_LIVE_ROOT": str(live_root),
            "AEROBAG_RELEASE_LIVE_SCRATCH": str(scratch_root),
            releases.RELEASE_LIVE_FEEDS_STATE_ENV: str(state_root),
            "AEROBAG_RELEASE_FETCH_CACHE": str(self.artifact_root / "cache/fetch"),
            "AEROBAG_RELEASE_PRODUCT_ARTIFACTS": str(Path(record.product_manifest).resolve()),
        }
        environment.write_text(
            "".join(f"{key}={json.dumps(value)}\n" for key, value in values.items()),
            encoding="utf-8",
        )
        unit = f"aerobag-live-feeds-release@{tag}.service"
        _run(["systemctl", "daemon-reload"])
        _run(["systemctl", "enable", "--now", unit])
        status_url = f"{record.live_feed_endpoint}/live-feeds/status.json"
        last_error = None
        for _ in range(60):
            try:
                with urllib.request.urlopen(status_url, timeout=2) as response:
                    if response.status == 200:
                        record.live_feed_status = "running"
                        record.draining_until_utc = None
                        record.last_error = None
                        self.save()
                        return
            except OSError as error:
                last_error = error
            time.sleep(1)
        detail = service_failure_detail(unit)
        _run(["systemctl", "disable", "--now", unit])
        record.last_error = (
            f"live-feed health did not become ready: {detail}"
            if detail is not None
            else f"live-feed health did not become ready: {last_error}"
        )
        record.live_feed_status = "failed"
        self.save()
        raise RuntimeError(record.last_error)

    def _manifest(self, tag: str) -> releases.ChannelManifest:
        path = self.observed.releases[tag].product_manifest
        if path is None:
            raise RuntimeError(f"release {tag} has no product manifest")
        return releases.load_channel_manifest(tag, Path(path))

    def ready_staging_tag(self) -> str | None:
        if self.desired.staging is None:
            return None
        tag = self.desired.staging.tag
        candidate = self.observed.releases[tag]
        if (
            candidate.build_status == "passed"
            and candidate.live_feed_endpoint is not None
            and candidate.live_feed_status == "running"
        ):
            return tag
        return None

    def controller_preprocessor(self) -> Path:
        # Merge and GC implement the deployed controller's publication contract,
        # not any client release's runtime contract.
        return self.args.controller_preprocessor.resolve()

    def record_qualification_bypass(self, bypass: dict | None) -> None:
        if bypass is None:
            return
        record = self.observed.releases[self.desired.production.tag]
        record.qualification_status = "bypassed"
        record.qualification_record = None
        record.qualification_bypassed_at_utc = bypass["at_utc"]
        record.qualification_bypass_reason = bypass["reason"]

    def activate(self) -> None:
        self.progress("Switching release channels")
        production_tags = [
            self.desired.production.tag,
            *[binding.tag for binding in self.desired.sunset],
        ]
        production = [self._manifest(tag) for tag in production_tags]
        staging_tag = self.ready_staging_tag()
        staging = [] if staging_tag is None else [self._manifest(staging_tag)]
        all_tags = set(production_tags)
        if staging_tag is not None:
            all_tags.add(staging_tag)
        assets = {}
        for tag in all_tags:
            record = self.observed.releases[tag]
            if record.release_root is None or record.live_feed_endpoint is None:
                raise RuntimeError(f"release {tag} is not ready for activation")
            release_root = Path(record.release_root)
            release_builder.normalize_release_permissions(release_root)
            release_builder.validate_release_directory(
                release_root, record.tag, record.commit
            )
            assets[tag] = releases.ReleaseAssets(
                release_root, record.live_feed_endpoint
            )

        generation_number = self.observed.generation + 1
        generation = self.artifact_root / "channel-generations" / f"{generation_number:08d}"
        current_link = self.artifact_root / "channel-current"
        if generation.exists():
            if current_link.is_symlink() and current_link.resolve() == generation.resolve():
                raise RuntimeError(
                    f"observed state does not account for active generation {generation}"
                )
            shutil.rmtree(generation)
        releases.materialize_channel_generation(
            generation,
            self.artifact_root / "published",
            production_manifests=production,
            staging_manifests=staging,
            release_assets=assets,
        )
        controller_preprocessor = self.controller_preprocessor()

        def merge_channel(
            manifests: list[releases.ChannelManifest], output: Path
        ) -> None:
            command = [
                str(controller_preprocessor),
                "merge-current-artifacts",
                "--build-root",
                str(self.artifact_root),
                "--output",
                str(output),
            ]
            for manifest in releases.discovery_manifests(manifests):
                command.extend(["--manifest", str(manifest.source_path)])
            _run(command)

        merge_channel(
            production,
            generation / "production/packages/current_artifacts.json",
        )
        if staging:
            merge_channel(
                staging,
                generation / "staging/packages/current_artifacts.json",
            )
        qualification_bypass = None
        if (
            self.observed.production != self.desired.production.tag
            and self.args.force_production_tag == self.desired.production.tag
        ):
            qualification_bypass = {
                "at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
                "reason": "forced promotion",
            }
        (generation / "generation.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "generation": generation_number,
                    "production": self.desired.production.tag,
                    "staging": staging_tag,
                    "sunset": [binding.tag for binding in self.desired.sunset],
                    # Preserve the audit if activation completes just before
                    # a controller restart prevents saving observed state.
                    "qualification_bypass": qualification_bypass,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        (generation / "live-feeds.nginx.conf").write_text(
            releases.render_live_feed_nginx_routes(
                production_endpoint=assets[self.desired.production.tag].live_feed_endpoint,
                staging_endpoint=(
                    None
                    if staging_tag is None
                    else assets[staging_tag].live_feed_endpoint
                ),
                release_endpoints={
                    tag: asset.live_feed_endpoint for tag, asset in assets.items()
                },
            ),
            encoding="utf-8",
        )
        previous = (
            (self.artifact_root / "channel-current").resolve()
            if (self.artifact_root / "channel-current").is_symlink()
            else None
        )
        releases.activate_channel_generation(self.artifact_root, generation)
        try:
            _run(["nginx", "-t"])
            _run(["systemctl", "reload", "nginx.service"])
            self.validate_public_production()
        except BaseException:
            if previous is not None:
                releases.activate_channel_generation(self.artifact_root, previous)
                _run(["systemctl", "reload", "nginx.service"])
            raise
        self.record_qualification_bypass(qualification_bypass)
        self.observed.production = self.desired.production.tag
        self.observed.staging = (
            staging_tag
        )
        self.observed.sunset = [binding.tag for binding in self.desired.sunset]
        self.observed.generation = generation_number
        self.observed.channel_inputs_dirty = False
        self.observed.gc_pending = True
        self.invalidate_deployment_checks()
        draining_deadline = (
            datetime.now(timezone.utc) + releases.RELEASE_DRAIN_GRACE
        ).isoformat().replace("+00:00", "Z")
        if previous is not None and previous.name == "legacy-bootstrap":
            self.observed.legacy_live_feed_draining_until_utc = draining_deadline
        for tag, record in self.observed.releases.items():
            if tag not in all_tags and record.live_feed_status == "running" and record.draining_until_utc is None:
                record.draining_until_utc = draining_deadline
        self.save()

    def invalidate_deployment_checks(self) -> None:
        tags = [self.observed.production, self.observed.staging, *self.observed.sunset]
        for tag in filter(None, tags):
            record = self.observed.releases[tag]
            record.deployment_status = "pending"
            record.deployment_error = None

    def check_deployment(self, tag: str) -> dict:
        role, channel = deployment_channel(self.observed, tag)
        self.progress(f"Running {role} deployment checks for {tag}")
        record = self.observed.releases[tag]
        release_root = Path(record.release_root or "")
        try:
            evidence = deployment_evidence(
                record, self.observed, self.artifact_root, self.args.public_origin
            )
            responses = check_public_responses(
                public_channel_checks(
                    self.args.public_origin,
                    role,
                    channel,
                    self.artifact_root / "channel-current" / channel,
                    release_root,
                ),
                f"{role} deployment checks for {tag}",
            )
            if evidence != deployment_evidence(
                record, self.observed, self.artifact_root, self.args.public_origin
            ):
                raise RuntimeError(f"deployment artifacts changed during checks for {tag}")
            receipt = {
                **evidence,
                "checked_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
                "public_response_sha256": responses,
            }
            path = self.artifact_root / "state/deployment-checks" / tag / f"{role}.json"
            releases._write_json_atomic(path, receipt)
        except Exception as error:
            record.deployment_status = "failed"
            record.deployment_error = str(error)
            self.save()
            raise
        record.deployment_record = str(path)
        record.deployment_status = "passed"
        record.deployment_error = None
        self.save()
        return receipt

    def qualify(self, tag: str) -> None:
        if self.observed.staging != tag:
            raise RuntimeError(f"release {tag} must be active on staging before qualification")
        record = self.observed.releases[tag]
        try:
            receipt = self.check_deployment(tag)
        except Exception as error:
            record.qualification_status = "failed"
            record.last_error = str(error)
            self.save()
            raise
        release_root = Path(record.release_root or "")
        qualification = {
            "schema_version": 1,
            "tag": tag,
            "commit": record.commit,
            "qualified_at_utc": datetime.now(timezone.utc).isoformat().replace(
                "+00:00", "Z"
            ),
            "release_json_sha256": _sha256(release_root / "release.json"),
            "product_manifest_sha256": _sha256(Path(record.product_manifest or "")),
            "public_response_sha256": receipt["public_response_sha256"],
        }
        qualification_path = release_root / "qualification.json"
        qualification_path.write_text(
            json.dumps(qualification, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        record.qualification_record = str(qualification_path)
        record.qualification_status = "passed"
        record.qualification_bypassed_at_utc = None
        record.qualification_bypass_reason = None
        record.last_error = None
        self.save()

    def check_active_deployments(self) -> None:
        if not releases.deployment_checks_are_only_pending_work(
            self.desired, self.observed
        ):
            raise RuntimeError("release state must converge before deployment-only checks")
        for tag in self.desired.tags():
            self.check_deployment(tag)
        self.progress("Deployment checks complete")

    def adopt_legacy_production_if_exact(self) -> bool:
        """Record the pre-controller deployment without weakening later gates."""

        if self.observed.production is not None or self.observed.generation != 0:
            return False
        tag = self.desired.production.tag
        record = self.observed.releases[tag]
        if (
            record.build_status != "passed"
            or record.live_feed_endpoint is None
            or record.live_feed_status != "running"
        ):
            return False
        try:
            deployed_commit = self.args.legacy_deployed_rev_file.read_text(
                encoding="utf-8"
            ).strip()
        except OSError:
            return False
        if deployed_commit != record.commit:
            return False
        release_root = Path(record.release_root or "")
        qualification = {
            "schema_version": 1,
            "kind": "legacy-production-adoption",
            "tag": tag,
            "commit": record.commit,
            "qualified_at_utc": datetime.now(timezone.utc).isoformat().replace(
                "+00:00", "Z"
            ),
            "evidence": str(self.args.legacy_deployed_rev_file),
            "release_json_sha256": _sha256(release_root / "release.json"),
            "product_manifest_sha256": _sha256(Path(record.product_manifest or "")),
        }
        qualification_path = release_root / "qualification.json"
        qualification_path.write_text(
            json.dumps(qualification, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        record.qualification_record = str(qualification_path)
        record.qualification_status = "passed"
        record.legacy_adopted = True
        self.observed.production = tag
        self.observed.sunset = [binding.tag for binding in self.desired.sunset]
        self.save()
        return True

    def reconcile(self, *, plan_only: bool) -> int:
        for _ in range(100):
            self.adopt_legacy_production_if_exact()
            plan = releases.plan_reconciliation(
                self.desired,
                self.observed,
                force_production_tag=self.args.force_production_tag,
            )
            print(
                json.dumps(
                    {
                        "actions": [action.__dict__ for action in plan.actions],
                        "blocked_reason": plan.blocked_reason,
                        "force_production_tag": self.args.force_production_tag,
                    },
                    sort_keys=True,
                ),
                flush=True,
            )
            if plan_only or plan.converged:
                if not plan_only:
                    self.progress("Release reconciliation complete")
                return 0
            if not plan.actions:
                print(f"release reconciliation blocked: {plan.blocked_reason}", file=sys.stderr)
                return 2
            action = plan.actions[0]
            if action.kind == "build_release" and action.tag is not None:
                self.build(action.tag)
            elif action.kind == "start_live_feeds" and action.tag is not None:
                self.start_live_feeds(action.tag)
            elif action.kind == "activate_generation":
                self.activate()
            elif action.kind == "qualify_release" and action.tag is not None:
                self.qualify(action.tag)
            elif action.kind == "check_deployment" and action.tag is not None:
                self.check_deployment(action.tag)
            else:
                raise RuntimeError(f"unsupported reconciliation action {action}")
        raise RuntimeError("release reconciliation did not converge after 100 actions")


def main() -> int:
    args = parse_args()
    args.artifact_root.mkdir(parents=True, exist_ok=True)
    lock_path = args.artifact_root / "locks/release-reconciler.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a+", encoding="utf-8") as lock:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit("another release reconciliation is already running") from None
        controller = Controller(args)
        if args.check_deployments_only:
            controller.check_active_deployments()
            return 0
        # Failed/stale serving checks must not starve periodic product refresh:
        # the refresh may itself repair the bad publication. Only structural
        # release work (build/activation/staging admission) defers maintenance.
        assignment_pending = not releases.deployment_checks_are_only_pending_work(
            controller.desired, controller.observed,
        )
        run_gc, refresh_products = maintenance_policy(
            assignment_pending=assignment_pending,
            refresh_requested=args.refresh_products,
        )
        if not args.plan:
            controller.save()
        if not args.plan:
            controller.recover_activated_generation()
            controller.maintain_retirement()
            if run_gc:
                controller.run_pending_gc()
        if refresh_products and not args.plan:
            controller.refresh_products()
        return controller.reconcile(plan_only=args.plan)


if __name__ == "__main__":
    raise SystemExit(main())
