#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Desired-state release validation and reconciliation primitives.

This module is deliberately independent of SSH, systemd, and nginx. The prod
deployment adapter executes the actions produced here; tests can therefore
prove release semantics without mutating a host.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlparse


DESIRED_SCHEMA_VERSION = 1
OBSERVED_SCHEMA_VERSION = 1
CHANNEL_GENERATION_SCHEMA_VERSION = 1
RELEASE_LIVE_FEEDS_STATE_ENV = "AEROBAG_RELEASE_LIVE_FEEDS_STATE_ROOT"
RECONCILIATION_PROGRESS_RELATIVE_PATH = "state/release-reconciliation-progress"
RELEASE_TAG_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")


class ReleaseConfigError(ValueError):
    pass


def _object(value: Any, context: str, fields: set[str]) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ReleaseConfigError(f"{context} must be an object")
    unknown = sorted(set(value) - fields)
    if unknown:
        raise ReleaseConfigError(
            f"{context} has unknown field{'s' if len(unknown) != 1 else ''}: "
            + ", ".join(unknown)
        )
    return value


def _release_tag(value: Any, context: str) -> str:
    if not isinstance(value, str) or not RELEASE_TAG_RE.fullmatch(value):
        raise ReleaseConfigError(f"{context} must be a safe non-empty Git tag")
    return value


def _parse_utc(value: Any, context: str) -> str:
    if not isinstance(value, str):
        raise ReleaseConfigError(f"{context} must be an RFC3339 timestamp")
    normalized = value[:-1] + "+00:00" if value.endswith("Z") else value
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError as error:
        raise ReleaseConfigError(f"{context} must be an RFC3339 timestamp") from error
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise ReleaseConfigError(f"{context} must include a timezone")
    return value


@dataclass(frozen=True)
class ReleaseBinding:
    tag: str


@dataclass(frozen=True)
class SunsetBinding:
    tag: str
    until_utc: str


@dataclass(frozen=True)
class DesiredReleases:
    production: ReleaseBinding
    staging: ReleaseBinding | None
    sunset: tuple[SunsetBinding, ...]

    def tags(self) -> tuple[str, ...]:
        ordered = [self.production.tag]
        if self.staging is not None and self.staging.tag not in ordered:
            ordered.append(self.staging.tag)
        for binding in self.sunset:
            if binding.tag not in ordered:
                ordered.append(binding.tag)
        return tuple(ordered)


def effective_desired_releases(
    desired: DesiredReleases,
    now_utc: datetime | None = None,
) -> DesiredReleases:
    now = now_utc or datetime.now(timezone.utc)
    if now.tzinfo is None or now.utcoffset() is None:
        raise ReleaseConfigError("effective release time must include a timezone")
    active = []
    for binding in desired.sunset:
        normalized = (
            binding.until_utc[:-1] + "+00:00"
            if binding.until_utc.endswith("Z")
            else binding.until_utc
        )
        if datetime.fromisoformat(normalized) > now:
            active.append(binding)
    return DesiredReleases(
        production=desired.production,
        staging=desired.staging,
        sunset=tuple(active),
    )


def _parse_binding(value: Any, context: str) -> ReleaseBinding:
    document = _object(value, context, {"tag"})
    if "tag" not in document:
        raise ReleaseConfigError(f"{context}.tag is required")
    return ReleaseBinding(tag=_release_tag(document["tag"], f"{context}.tag"))


def parse_desired_releases(value: Any) -> DesiredReleases:
    document = _object(
        value,
        "release desired state",
        {"schema_version", "production", "staging", "sunset"},
    )
    if document.get("schema_version") != DESIRED_SCHEMA_VERSION:
        raise ReleaseConfigError(
            f"release desired state requires schema_version {DESIRED_SCHEMA_VERSION}"
        )
    if "production" not in document:
        raise ReleaseConfigError("release desired state production is required")
    production = _parse_binding(document["production"], "production")
    staging_value = document.get("staging")
    staging = (
        None if staging_value is None else _parse_binding(staging_value, "staging")
    )
    sunset_value = document.get("sunset", [])
    if not isinstance(sunset_value, list):
        raise ReleaseConfigError("sunset must be an array")
    sunset = []
    seen_sunset: set[str] = set()
    for index, item in enumerate(sunset_value):
        context = f"sunset[{index}]"
        entry = _object(item, context, {"tag", "until_utc"})
        if "tag" not in entry or "until_utc" not in entry:
            raise ReleaseConfigError(f"{context} requires tag and until_utc")
        tag = _release_tag(entry["tag"], f"{context}.tag")
        if tag in seen_sunset:
            raise ReleaseConfigError(f"sunset contains duplicate release {tag}")
        seen_sunset.add(tag)
        sunset.append(
            SunsetBinding(
                tag=tag,
                until_utc=_parse_utc(entry["until_utc"], f"{context}.until_utc"),
            )
        )
    if production.tag in seen_sunset:
        raise ReleaseConfigError(
            f"production release {production.tag} must not also appear in sunset"
        )
    if staging is not None and staging.tag in seen_sunset:
        raise ReleaseConfigError(
            f"staging release {staging.tag} must not also appear in sunset"
        )
    if staging is not None and staging.tag == production.tag:
        raise ReleaseConfigError(
            f"release {production.tag} must not be assigned to production and staging"
        )
    return DesiredReleases(
        production=production,
        staging=staging,
        sunset=tuple(sunset),
    )


def load_desired_releases(path: Path) -> DesiredReleases:
    try:
        return parse_desired_releases(json.loads(path.read_text(encoding="utf-8")))
    except OSError as error:
        raise ReleaseConfigError(f"failed to read release desired state {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ReleaseConfigError(f"invalid JSON in release desired state {path}: {error}") from error


@dataclass(frozen=True)
class ResolvedTag:
    tag: str
    tag_object: str
    commit: str


def _git(repo_root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ReleaseConfigError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def resolve_release_tag(repo_root: Path, tag: str) -> ResolvedTag:
    tag = _release_tag(tag, "release tag")
    ref = f"refs/tags/{tag}"
    object_type = _git(repo_root, "cat-file", "-t", ref)
    if object_type != "tag":
        raise ReleaseConfigError(f"release {tag} must be an annotated Git tag")
    tag_object = _git(repo_root, "rev-parse", ref)
    commit = _git(repo_root, "rev-parse", f"{ref}^{{commit}}")
    if not COMMIT_RE.fullmatch(tag_object) or not COMMIT_RE.fullmatch(commit):
        raise ReleaseConfigError(f"release {tag} resolved to invalid Git object ids")
    return ResolvedTag(tag=tag, tag_object=tag_object, commit=commit)


def resolve_desired_tags(repo_root: Path, desired: DesiredReleases) -> dict[str, ResolvedTag]:
    return {tag: resolve_release_tag(repo_root, tag) for tag in desired.tags()}


@dataclass
class ObservedRelease:
    tag: str
    tag_object: str
    commit: str
    build_status: str = "pending"
    qualification_status: str = "pending"
    product_manifest: str | None = None
    release_root: str | None = None
    live_feed_endpoint: str | None = None
    live_feed_status: str = "pending"
    qualification_record: str | None = None
    last_error: str | None = None
    draining_until_utc: str | None = None
    legacy_adopted: bool = False

    @classmethod
    def from_dict(cls, value: Any, context: str) -> "ObservedRelease":
        document = _object(
            value,
            context,
            {
                "tag",
                "tag_object",
                "commit",
                "build_status",
                "qualification_status",
                "product_manifest",
                "release_root",
                "live_feed_endpoint",
                "live_feed_status",
                "qualification_record",
                "last_error",
                "draining_until_utc",
                "legacy_adopted",
            },
        )
        required = {"tag", "tag_object", "commit", "build_status", "qualification_status"}
        missing = sorted(required - set(document))
        if missing:
            raise ReleaseConfigError(f"{context} is missing {', '.join(missing)}")
        return cls(**document)


@dataclass
class ObservedState:
    releases: dict[str, ObservedRelease] = field(default_factory=dict)
    production: str | None = None
    staging: str | None = None
    sunset: list[str] = field(default_factory=list)
    generation: int = 0
    desired_commit: str | None = None
    channel_inputs_dirty: bool = False
    legacy_live_feed_draining_until_utc: str | None = None
    gc_pending: bool = False

    @classmethod
    def empty(cls) -> "ObservedState":
        return cls()

    @classmethod
    def from_dict(cls, value: Any) -> "ObservedState":
        document = _object(
            value,
            "release observed state",
            {
                "schema_version",
                "releases",
                "production",
                "staging",
                "sunset",
                "generation",
                "desired_commit",
                "channel_inputs_dirty",
                "legacy_live_feed_draining_until_utc",
                "gc_pending",
            },
        )
        if document.get("schema_version") != OBSERVED_SCHEMA_VERSION:
            raise ReleaseConfigError(
                f"release observed state requires schema_version {OBSERVED_SCHEMA_VERSION}"
            )
        releases_value = document.get("releases", {})
        if not isinstance(releases_value, dict):
            raise ReleaseConfigError("release observed state releases must be an object")
        parsed = {
            tag: ObservedRelease.from_dict(record, f"releases[{tag!r}]")
            for tag, record in releases_value.items()
        }
        for tag, record in parsed.items():
            if tag != record.tag:
                raise ReleaseConfigError(f"release key {tag} does not match record tag {record.tag}")
        sunset = document.get("sunset", [])
        if not isinstance(sunset, list) or not all(isinstance(tag, str) for tag in sunset):
            raise ReleaseConfigError("release observed state sunset must be a string array")
        generation = document.get("generation", 0)
        if not isinstance(generation, int) or generation < 0:
            raise ReleaseConfigError("release observed state generation must be non-negative")
        return cls(
            releases=parsed,
            production=document.get("production"),
            staging=document.get("staging"),
            sunset=list(sunset),
            generation=generation,
            desired_commit=document.get("desired_commit"),
            channel_inputs_dirty=document.get("channel_inputs_dirty", False),
            legacy_live_feed_draining_until_utc=document.get(
                "legacy_live_feed_draining_until_utc"
            ),
            gc_pending=document.get("gc_pending", False),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": OBSERVED_SCHEMA_VERSION,
            "releases": {
                tag: asdict(record) for tag, record in sorted(self.releases.items())
            },
            "production": self.production,
            "staging": self.staging,
            "sunset": list(self.sunset),
            "generation": self.generation,
            "desired_commit": self.desired_commit,
            "channel_inputs_dirty": self.channel_inputs_dirty,
            "legacy_live_feed_draining_until_utc": (
                self.legacy_live_feed_draining_until_utc
            ),
            "gc_pending": self.gc_pending,
        }


def load_observed_state(path: Path) -> ObservedState:
    if not path.exists():
        return ObservedState.empty()
    try:
        return ObservedState.from_dict(json.loads(path.read_text(encoding="utf-8")))
    except OSError as error:
        raise ReleaseConfigError(f"failed to read release observed state {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ReleaseConfigError(f"invalid JSON in release observed state {path}: {error}") from error


def write_observed_state(path: Path, state: ObservedState) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(state.to_dict(), indent=2, sort_keys=True) + "\n"
    fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temp_path = Path(temp_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, path)
    finally:
        temp_path.unlink(missing_ok=True)


def verify_release_identity(resolved: ResolvedTag, observed: ObservedRelease) -> None:
    if resolved.tag != observed.tag:
        raise ReleaseConfigError(
            f"resolved release {resolved.tag} does not match observed {observed.tag}"
        )
    if resolved.tag_object != observed.tag_object:
        raise ReleaseConfigError(
            f"release {resolved.tag} changed tag object from {observed.tag_object} "
            f"to {resolved.tag_object}"
        )
    if resolved.commit != observed.commit:
        raise ReleaseConfigError(
            f"release {resolved.tag} changed commit from {observed.commit} to {resolved.commit}"
        )


@dataclass(frozen=True)
class ReconcileAction:
    kind: str
    tag: str | None = None


@dataclass(frozen=True)
class ReconciliationPlan:
    actions: list[ReconcileAction]
    blocked_reason: str | None = None

    @property
    def converged(self) -> bool:
        return not self.actions and self.blocked_reason is None


def _desired_sunset_tags(desired: DesiredReleases) -> list[str]:
    return [binding.tag for binding in desired.sunset]


def plan_reconciliation(
    desired: DesiredReleases,
    observed: ObservedState,
    *,
    force_production_tag: str | None = None,
) -> ReconciliationPlan:
    production_tags = [desired.production.tag, *_desired_sunset_tags(desired)]
    for tag in production_tags:
        record = observed.releases.get(tag)
        if record is None or record.build_status != "passed":
            return ReconciliationPlan([ReconcileAction("build_release", tag)])
        if record.live_feed_endpoint is None or record.live_feed_status != "running":
            return ReconciliationPlan([ReconcileAction("start_live_feeds", tag)])

    production_tag = desired.production.tag
    if observed.production != production_tag:
        production_record = observed.releases[production_tag]
        force_activation = force_production_tag == production_tag
        if production_record.qualification_status != "passed" and not force_activation:
            if observed.staging != production_tag:
                return ReconciliationPlan(
                    [],
                    f"production release {production_tag} has not been qualified on staging",
                )
            return ReconciliationPlan(
                [ReconcileAction("qualify_release", production_tag)],
                f"production release {production_tag} is not qualified",
            )
        if force_activation and observed.staging != production_tag:
            return ReconciliationPlan(
                [],
                f"forced production release {production_tag} is not active on staging",
            )
        return ReconciliationPlan([ReconcileAction("activate_generation", production_tag)])

    if observed.channel_inputs_dirty or observed.generation == 0:
        return ReconciliationPlan([ReconcileAction("activate_generation")])

    staging_tag = desired.staging.tag if desired.staging is not None else None
    if staging_tag is not None:
        staging_record = observed.releases.get(staging_tag)
        if staging_record is None or staging_record.build_status != "passed":
            return ReconciliationPlan([ReconcileAction("build_release", staging_tag)])
        if (
            staging_record.live_feed_endpoint is None
            or staging_record.live_feed_status != "running"
        ):
            return ReconciliationPlan([ReconcileAction("start_live_feeds", staging_tag)])
        if observed.staging != staging_tag:
            return ReconciliationPlan([ReconcileAction("activate_generation", staging_tag)])
        if staging_record.qualification_status != "passed":
            return ReconciliationPlan([ReconcileAction("qualify_release", staging_tag)])

    desired_sunset = _desired_sunset_tags(desired)
    if observed.staging != staging_tag or observed.sunset != desired_sunset:
        return ReconciliationPlan([ReconcileAction("activate_generation")])
    return ReconciliationPlan([])


@dataclass(frozen=True)
class ChannelManifest:
    release_tag: str
    source_path: Path
    document: dict[str, Any]
    publication_roots: tuple[str, ...]


def load_channel_manifest(release_tag: str, source_path: Path) -> ChannelManifest:
    try:
        document = json.loads(source_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseConfigError(
            f"failed to load product manifest {source_path}: {error}"
        ) from error
    document = _object(
        document,
        f"product manifest {source_path}",
        {
            "schema_version",
            "contracts",
            "artifact_roots",
            "as_of_date",
            "as_of_utc",
            "bundles",
            "startup_prefetch",
            "diagnostics",
        },
    )
    roots = document.get("artifact_roots")
    if not isinstance(roots, dict):
        raise ReleaseConfigError(f"product manifest {source_path} has no artifact_roots")
    publication_roots = set()
    for field in ("packaged", "unpacked"):
        value = roots.get(field)
        if not isinstance(value, str):
            raise ReleaseConfigError(
                f"product manifest {source_path} has invalid artifact_roots.{field}"
            )
        relative = Path(value)
        if relative.is_absolute() or ".." in relative.parts or not relative.parts:
            raise ReleaseConfigError(
                f"product manifest {source_path} has unsafe artifact_roots.{field}"
            )
        publication_roots.add(_safe_publication_root(relative.parts[0]))
    return ChannelManifest(
        release_tag=release_tag,
        source_path=source_path,
        document=document,
        publication_roots=tuple(sorted(publication_roots)),
    )


@dataclass(frozen=True)
class ReleaseAssets:
    release_root: Path
    live_feed_endpoint: str


def _link_directory(link: Path, target: Path) -> None:
    if not target.is_dir():
        raise ReleaseConfigError(f"release asset directory does not exist: {target}")
    link.parent.mkdir(parents=True, exist_ok=True)
    link.symlink_to(os.path.relpath(target, link.parent), target_is_directory=True)


def _safe_publication_root(value: str) -> str:
    path = Path(value)
    if (
        not value
        or path.is_absolute()
        or len(path.parts) != 1
        or path.parts[0] in {".", ".."}
    ):
        raise ReleaseConfigError(f"invalid publication root {value!r}")
    return value


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _write_json_atomic(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(value, indent=2, sort_keys=True) + "\n"
    fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temp_path = Path(temp_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, path)
    finally:
        temp_path.unlink(missing_ok=True)


def _link_publication_roots(
    packages_root: Path,
    published_root: Path,
    manifests: Iterable[ChannelManifest],
) -> None:
    packages_root.mkdir(parents=True, exist_ok=True)
    linked: set[str] = set()
    for manifest in manifests:
        for raw_root in manifest.publication_roots:
            root = _safe_publication_root(raw_root)
            if root in linked:
                continue
            target = published_root / root
            if not target.is_dir():
                raise ReleaseConfigError(f"publication root does not exist: {target}")
            relative_target = os.path.relpath(target, packages_root)
            (packages_root / root).symlink_to(relative_target, target_is_directory=True)
            linked.add(root)


def materialize_channel_generation(
    output_root: Path,
    published_root: Path,
    *,
    production_manifests: list[ChannelManifest],
    staging_manifests: list[ChannelManifest],
    release_assets: dict[str, ReleaseAssets] | None = None,
) -> None:
    if output_root.exists():
        raise ReleaseConfigError(f"channel generation already exists: {output_root}")
    if not production_manifests:
        raise ReleaseConfigError("production channel requires at least one manifest")
    output_root.mkdir(parents=True)
    try:
        release_assets = release_assets or {}
        channels = {
            "production": production_manifests,
            "staging": staging_manifests,
        }
        current_artifacts_paths = []
        for channel, manifests in channels.items():
            if not manifests:
                continue
            packages_root = output_root / channel / "packages"
            current_path = packages_root / "current_artifacts.json"
            _write_json(current_path, [manifest.document for manifest in manifests])
            _link_publication_roots(packages_root, published_root, manifests)
            current_artifacts_paths.append(
                current_path.relative_to(output_root).as_posix()
            )
            controlling_tag = manifests[0].release_tag
            assets = release_assets.get(controlling_tag)
            if assets is not None:
                _link_directory(
                    output_root / channel / "web", assets.release_root / "web"
                )
                _link_directory(
                    output_root / channel / "downloads",
                    assets.release_root / "downloads",
                )

        release_manifests: dict[str, ChannelManifest] = {}
        for manifest in [*production_manifests, *staging_manifests]:
            release_manifests[manifest.release_tag] = manifest
        for tag, manifest in sorted(release_manifests.items()):
            packages_root = output_root / "releases" / tag / "packages"
            current_path = packages_root / "current_artifacts.json"
            _write_json(current_path, [manifest.document])
            _link_publication_roots(packages_root, published_root, [manifest])
            current_artifacts_paths.append(
                current_path.relative_to(output_root).as_posix()
            )
            assets = release_assets.get(tag)
            if assets is not None:
                _link_directory(
                    output_root / "releases" / tag / "web",
                    assets.release_root / "web",
                )
                _link_directory(
                    output_root / "releases" / tag / "downloads",
                    assets.release_root / "downloads",
                )

        _write_json(
            output_root / "live-feed-routes.json",
            {
                "schema_version": CHANNEL_GENERATION_SCHEMA_VERSION,
                "production": (
                    release_assets[production_manifests[0].release_tag].live_feed_endpoint
                    if production_manifests[0].release_tag in release_assets
                    else None
                ),
                "staging": (
                    release_assets[staging_manifests[0].release_tag].live_feed_endpoint
                    if staging_manifests
                    and staging_manifests[0].release_tag in release_assets
                    else None
                ),
                "releases": {
                    tag: assets.live_feed_endpoint
                    for tag, assets in sorted(release_assets.items())
                    if tag in release_manifests
                },
            },
        )

        _write_json(
            output_root / "gc-root-manifests.json",
            {
                "schema_version": CHANNEL_GENERATION_SCHEMA_VERSION,
                "current_artifacts_paths": sorted(set(current_artifacts_paths)),
            },
        )
    except BaseException:
        # The caller owns removal of an abandoned generation. Never publish a
        # partial tree by installing channel-current.
        raise


def _generation_gc_paths(build_root: Path, generation_root: Path) -> list[str]:
    try:
        relative_generation = generation_root.relative_to(build_root)
    except ValueError as error:
        raise ReleaseConfigError(
            f"channel generation must be under build root {build_root}: {generation_root}"
        ) from error
    registry_path = generation_root / "gc-root-manifests.json"
    try:
        document = json.loads(registry_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReleaseConfigError(
            f"failed to load channel generation roots {registry_path}: {error}"
        ) from error
    document = _object(
        document,
        f"channel generation roots {registry_path}",
        {"schema_version", "current_artifacts_paths"},
    )
    if document.get("schema_version") != CHANNEL_GENERATION_SCHEMA_VERSION:
        raise ReleaseConfigError(
            f"channel generation roots require schema_version "
            f"{CHANNEL_GENERATION_SCHEMA_VERSION}"
        )
    values = document.get("current_artifacts_paths")
    if not isinstance(values, list) or not values:
        raise ReleaseConfigError(
            f"channel generation roots {registry_path} must contain paths"
        )
    result = []
    for value in values:
        if not isinstance(value, str):
            raise ReleaseConfigError(
                f"channel generation roots {registry_path} contains a non-string path"
            )
        relative = Path(value)
        if (
            relative.is_absolute()
            or any(part in {"", ".", ".."} for part in relative.parts)
            or relative.name != "current_artifacts.json"
        ):
            raise ReleaseConfigError(
                f"channel generation roots {registry_path} contains unsafe path {value!r}"
            )
        absolute = generation_root / relative
        if not absolute.is_file():
            raise ReleaseConfigError(
                f"channel generation roots {registry_path} references missing {absolute}"
            )
        result.append((relative_generation / relative).as_posix())
    return result


def activate_channel_generation(build_root: Path, generation_root: Path) -> None:
    """Atomically direct new requests at a complete generation.

    GC is rooted first and includes the prior generation. This may retain data
    briefly if the process stops between the two atomic writes, but it can
    never discard data still served by either side of a channel switch.
    """

    build_root = build_root.resolve()
    generation_root = generation_root.resolve()
    new_roots = _generation_gc_paths(build_root, generation_root)

    current_link = build_root / "channel-current"
    previous_roots: list[str] = []
    if current_link.is_symlink():
        previous_generation = current_link.resolve()
        if previous_generation != generation_root:
            previous_roots = _generation_gc_paths(build_root, previous_generation)

    _write_json_atomic(
        build_root / "state/release-gc-roots.json",
        {
            "schema_version": CHANNEL_GENERATION_SCHEMA_VERSION,
            "current_artifacts_paths": sorted(set([*new_roots, *previous_roots])),
        },
    )

    relative_target = os.path.relpath(generation_root, build_root)
    temporary_link = build_root / f".channel-current.{os.getpid()}"
    temporary_link.unlink(missing_ok=True)
    temporary_link.symlink_to(relative_target, target_is_directory=True)
    os.replace(temporary_link, current_link)


def _nginx_upstream(endpoint: str) -> str:
    parsed = urlparse(endpoint)
    if (
        parsed.scheme != "http"
        or parsed.hostname != "127.0.0.1"
        or parsed.port is None
        or parsed.path not in {"", "/"}
        or parsed.query
        or parsed.fragment
    ):
        raise ReleaseConfigError(
            f"live-feed endpoint must be a loopback HTTP origin: {endpoint!r}"
        )
    return f"http://127.0.0.1:{parsed.port}"


def render_live_feed_nginx_routes(
    *,
    production_endpoint: str,
    staging_endpoint: str | None,
    release_endpoints: dict[str, str],
) -> str:
    lines = [
        "# Generated by the Aerobag release reconciler; do not edit.",
        "location /live-feeds/ {",
        f"    proxy_pass {_nginx_upstream(production_endpoint)};",
        "    proxy_http_version 1.1;",
        "    proxy_buffering off;",
        "    proxy_read_timeout 1h;",
        "}",
    ]
    if staging_endpoint is not None:
        lines.extend(
            [
                "location /staging/live-feeds/ {",
                f"    proxy_pass {_nginx_upstream(staging_endpoint)}/live-feeds/;",
                "    proxy_http_version 1.1;",
                "    proxy_buffering off;",
                "    proxy_read_timeout 1h;",
                "}",
            ]
        )
    for tag, endpoint in sorted(release_endpoints.items()):
        tag = _release_tag(tag, "release live-feed route")
        lines.extend(
            [
                f"location /releases/{tag}/live-feeds/ {{",
                f"    proxy_pass {_nginx_upstream(endpoint)}/live-feeds/;",
                "    proxy_http_version 1.1;",
                "    proxy_buffering off;",
                "    proxy_read_timeout 1h;",
                "}",
            ]
        )
    return "\n".join(lines) + "\n"
