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

import hashlib
import json
import os
import re
import subprocess
import tempfile
from dataclasses import asdict, dataclass, field, replace
from datetime import datetime, timedelta, timezone
from enum import Enum
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlparse

import live_feed_compatibility as compatibility


DESIRED_SCHEMA_VERSION = 2
OBSERVED_SCHEMA_VERSION = 2
CHANNEL_GENERATION_SCHEMA_VERSION = 1
RELEASE_LIVE_FEEDS_STATE_ENV = "AEROBAG_RELEASE_LIVE_FEEDS_STATE_ROOT"
RECONCILIATION_PROGRESS_RELATIVE_PATH = "state/release-reconciliation-progress"
RELEASE_TAG_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
RELEASE_DRAIN_GRACE = timedelta(hours=1)
GENERATION_RETIREMENT_FILE = "retirement.json"
RELEASE_GC_ROOTS = "state/release-gc-roots.json"


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


class LiveFeedsPolicy(str, Enum):
    DEDICATED = "dedicated"
    SHARE_IF_COMPATIBLE = "share_if_compatible"


@dataclass(frozen=True)
class SunsetBinding:
    tag: str
    until_utc: str
    live_feeds: LiveFeedsPolicy = LiveFeedsPolicy.SHARE_IF_COMPATIBLE


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
    version = document.get("schema_version")
    if type(version) is not int or version not in (1, DESIRED_SCHEMA_VERSION):
        raise ReleaseConfigError(
            f"release desired state requires schema_version 1 or {DESIRED_SCHEMA_VERSION}"
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
        fields = {"tag", "until_utc"}
        if version == DESIRED_SCHEMA_VERSION:
            fields.add("live_feeds")
        entry = _object(item, context, fields)
        if "tag" not in entry or "until_utc" not in entry:
            raise ReleaseConfigError(f"{context} requires tag and until_utc")
        tag = _release_tag(entry["tag"], f"{context}.tag")
        if tag in seen_sunset:
            raise ReleaseConfigError(f"sunset contains duplicate release {tag}")
        seen_sunset.add(tag)
        try:
            policy = LiveFeedsPolicy(entry.get("live_feeds", "share_if_compatible"))
        except (ValueError, TypeError) as error:
            raise ReleaseConfigError(
                f"{context}.live_feeds must be dedicated or share_if_compatible"
            ) from error
        sunset.append(
            SunsetBinding(
                tag=tag,
                until_utc=_parse_utc(entry["until_utc"], f"{context}.until_utc"),
                live_feeds=policy,
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


def migrate_desired_releases(value: Any) -> dict[str, Any]:
    """Write explicit v2 policies without changing deadlines or caller input."""
    desired = parse_desired_releases(value)
    document = json.loads(json.dumps(value))
    document["schema_version"] = DESIRED_SCHEMA_VERSION
    for entry, binding in zip(document.get("sunset", []), desired.sunset):
        entry["live_feeds"] = binding.live_feeds.value
    return document


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
class LiveFeedInstance:
    instance_id: str
    release_tag: str
    launch_digest: str
    endpoint: str
    unit: str
    manifest: str
    roots: list[str] = field(default_factory=list)
    gc_paths: list[str] = field(default_factory=list)
    manifest_sha256: str | None = None
    evidence: dict[str, Any] | None = None
    status: str = "pending"
    draining_until_utc: str | None = None

    @classmethod
    def from_dict(cls, value: Any, context: str) -> "LiveFeedInstance":
        document = _object(value, context, set(cls.__dataclass_fields__))
        required = {
            "instance_id", "release_tag", "launch_digest", "endpoint", "unit", "manifest",
        }
        if required - document.keys():
            raise ReleaseConfigError(f"{context} is missing {', '.join(sorted(required - document.keys()))}")
        for name in required:
            if not isinstance(document[name], str) or not document[name].strip():
                raise ReleaseConfigError(f"{context}.{name} must be a non-empty string")
        _release_tag(document["release_tag"], f"{context}.release_tag")
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,159}", document["instance_id"]):
            raise ReleaseConfigError(f"{context}.instance_id must be a safe instance identifier")
        status = document.get("status", "pending")
        if not isinstance(status, str) or status not in {
            "pending", "running", "unavailable", "failed", "stopped", "removed",
        }:
            raise ReleaseConfigError(f"{context}.status is unknown")
        digest = document.get("manifest_sha256")
        if digest is not None and (not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest)):
            raise ReleaseConfigError(f"{context}.manifest_sha256 must be lowercase SHA-256 hex")
        for name in ("roots", "gc_paths"):
            roots = document.get(name, [])
            if not isinstance(roots, list) or any(not isinstance(root, str) or not root for root in roots):
                raise ReleaseConfigError(f"{context}.{name} must be a string array")
        if document.get("evidence") is not None and not isinstance(document["evidence"], dict):
            raise ReleaseConfigError(f"{context}.evidence must be an object or null")
        if document.get("draining_until_utc") is not None:
            _parse_utc(document["draining_until_utc"], f"{context}.draining_until_utc")
        return cls(**document)


@dataclass
class ObservedRelease:
    tag: str
    tag_object: str
    commit: str
    build_status: str = "pending"
    qualification_status: str = "pending"
    deployment_status: str = "pending"
    deployment_record: str | None = None
    deployment_error: str | None = None
    deployment_pending_since_utc: str | None = None
    product_refresh_status: str | None = None
    product_refresh_started_at_utc: str | None = None
    product_refresh_error: str | None = None
    product_manifest: str | None = None
    release_root: str | None = None
    live_feed_endpoint: str | None = None
    live_feed_status: str = "pending"
    live_feed_requirements: dict[str, Any] | None = None
    live_feed_instance: str | None = None
    live_feed_provider: str | None = None
    live_feed_reason: str | None = None
    qualification_record: str | None = None
    qualification_bypassed_at_utc: str | None = None
    qualification_bypass_reason: str | None = None
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
                "deployment_status",
                "deployment_record",
                "deployment_error",
                "deployment_pending_since_utc",
                "product_refresh_status",
                "product_refresh_started_at_utc",
                "product_refresh_error",
                "product_manifest",
                "release_root",
                "live_feed_endpoint",
                "live_feed_status",
                "live_feed_requirements",
                "live_feed_instance",
                "live_feed_provider",
                "live_feed_reason",
                "qualification_record",
                "qualification_bypassed_at_utc",
                "qualification_bypass_reason",
                "last_error",
                "draining_until_utc",
                "legacy_adopted",
            },
        )
        required = {"tag", "tag_object", "commit", "build_status", "qualification_status"}
        missing = sorted(required - set(document))
        if missing:
            raise ReleaseConfigError(f"{context} is missing {', '.join(missing)}")
        if document.get("live_feed_requirements") is not None and not isinstance(document["live_feed_requirements"], dict):
            raise ReleaseConfigError(f"{context}.live_feed_requirements must be an object or null")
        for name in ("live_feed_instance", "live_feed_provider", "live_feed_reason"):
            if document.get(name) is not None and (not isinstance(document[name], str) or not document[name]):
                raise ReleaseConfigError(f"{context}.{name} must be a non-empty string or null")
        return cls(**document)


@dataclass
class ObservedState:
    releases: dict[str, ObservedRelease] = field(default_factory=dict)
    live_feed_instances: dict[str, LiveFeedInstance] = field(default_factory=dict)
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
                "live_feed_instances",
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
        version = document.get("schema_version")
        if type(version) is not int or version not in (1, OBSERVED_SCHEMA_VERSION):
            raise ReleaseConfigError(
                f"release observed state requires schema_version 1 or {OBSERVED_SCHEMA_VERSION}"
            )
        if version == 1 and "live_feed_instances" in document:
            raise ReleaseConfigError("live_feed_instances requires observed schema_version 2")
        releases_value = document.get("releases", {})
        if not isinstance(releases_value, dict):
            raise ReleaseConfigError("release observed state releases must be an object")
        if version == 1:
            for record in releases_value.values():
                if isinstance(record, dict) and {
                    "live_feed_instance", "live_feed_provider", "live_feed_requirements", "live_feed_reason",
                } & record.keys():
                    raise ReleaseConfigError("live-feed instance metadata requires observed schema_version 2")
        parsed = {
            tag: ObservedRelease.from_dict(record, f"releases[{tag!r}]")
            for tag, record in releases_value.items()
        }
        for tag, record in parsed.items():
            if tag != record.tag:
                raise ReleaseConfigError(f"release key {tag} does not match record tag {record.tag}")
        instance_values = document.get("live_feed_instances", {})
        if not isinstance(instance_values, dict):
            raise ReleaseConfigError("release observed state live_feed_instances must be an object")
        instances = {
            key: LiveFeedInstance.from_dict(item, f"live_feed_instances[{key!r}]")
            for key, item in instance_values.items()
        }
        for key, instance in instances.items():
            if key != instance.instance_id:
                raise ReleaseConfigError(f"live feed instance key {key} does not match instance_id")
        for tag, record in parsed.items():
            for name in ("live_feed_instance", "live_feed_provider"):
                key = getattr(record, name)
                if key is not None and key not in instances:
                    raise ReleaseConfigError(f"releases[{tag!r}].{name} references missing instance {key}")
            if record.live_feed_instance is not None and instances[record.live_feed_instance].release_tag != tag:
                raise ReleaseConfigError(f"release {tag} own instance belongs to another release")
        sunset = document.get("sunset", [])
        if not isinstance(sunset, list) or not all(isinstance(tag, str) for tag in sunset):
            raise ReleaseConfigError("release observed state sunset must be a string array")
        generation = document.get("generation", 0)
        if not isinstance(generation, int) or generation < 0:
            raise ReleaseConfigError("release observed state generation must be non-negative")
        return cls(
            releases=parsed,
            live_feed_instances=instances,
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
            "live_feed_instances": {
                key: asdict(instance) for key, instance in sorted(self.live_feed_instances.items())
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


def live_feed_health_targets(
    desired: DesiredReleases, observed: ObservedState,
) -> dict[str, tuple[str, ...]]:
    """Attribute dependent releases to actual providers, not retained daemons."""
    targets: dict[str, list[str]] = {}
    for tag in desired.tags():
        record = observed.releases.get(tag)
        key = None if record is None else (record.live_feed_provider or record.live_feed_instance)
        instance = observed.live_feed_instances.get(key) if key is not None else None
        unit = instance.unit if instance is not None else f"aerobag-live-feeds-release@{tag}.service"
        targets.setdefault(unit, []).append(tag)
    return {unit: tuple(tags) for unit, tags in targets.items()}


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


@dataclass(frozen=True)
class ResolvedLiveFeedBinding:
    release_tag: str
    provider: str | None
    provider_tag: str
    endpoint: str | None
    reason: str
    verification: str

    @property
    def shared(self) -> bool:
        return self.provider_tag != self.release_tag


def dedicated_live_feed_ready(observed: ObservedState, tag: str) -> bool:
    record = observed.releases.get(tag)
    if record is None:
        return False
    if record.live_feed_instance is None:
        # Legacy fields describe only the release's own daemon, never its alias.
        return bool(record.live_feed_endpoint) and record.live_feed_status == "running"
    instance = observed.live_feed_instances.get(record.live_feed_instance)
    return (
        instance is not None and instance.release_tag == tag
        and bool(instance.endpoint) and instance.status == "running"
        and instance.draining_until_utc is None
    )


def _live_feed_binding(
    observed: ObservedState, tag: str, provider_tag: str, reason: str,
) -> ResolvedLiveFeedBinding:
    record = observed.releases.get(tag)
    provider = observed.releases.get(provider_tag)
    key = None if provider is None else provider.live_feed_instance
    instance = observed.live_feed_instances.get(key) if key is not None else None
    endpoint = (
        instance.endpoint if instance is not None else
        provider.live_feed_endpoint if provider is not None and key is None else None
    )
    # Freeze values, not references into mutable observations. Revalidation must
    # catch process restarts and evidence replacement even on an unchanged port.
    inputs = {
        "release": None if record is None else {
            "tag": record.tag, "commit": record.commit,
            "manifest": record.product_manifest, "requirements": record.live_feed_requirements,
        },
        "provider": None if provider is None else {
            "tag": provider.tag, "commit": provider.commit,
            "manifest": provider.product_manifest, "requirements": provider.live_feed_requirements,
        },
        "instance": None if instance is None else {
            **asdict(instance), "evidence": compatibility.verification_evidence(instance.evidence),
        },
        "legacy_endpoint": endpoint if key is None else None,
        "legacy_status": provider.live_feed_status if provider is not None and key is None else None,
    }
    verification = hashlib.sha256(json.dumps(inputs, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return ResolvedLiveFeedBinding(tag, key, provider_tag, endpoint, reason, verification)


def resolve_live_feed_bindings(
    desired: DesiredReleases, observed: ObservedState,
) -> dict[str, ResolvedLiveFeedBinding]:
    """Resolve proposed aliases without changing active bindings or doing I/O."""
    production_tag = desired.production.tag
    bindings = {
        production_tag: _live_feed_binding(observed, production_tag, production_tag, "dedicated: production"),
    }
    if desired.staging is not None:
        tag = desired.staging.tag
        bindings[tag] = _live_feed_binding(observed, tag, tag, "dedicated: staging isolation")
    production = observed.releases.get(production_tag)
    production_instance = (
        observed.live_feed_instances.get(production.live_feed_instance)
        if production is not None and production.live_feed_instance is not None else None
    )
    for sunset in desired.sunset:
        tag = sunset.tag
        provider_tag = tag
        if sunset.live_feeds == LiveFeedsPolicy.DEDICATED:
            reason = "dedicated: policy"
        elif not dedicated_live_feed_ready(observed, production_tag):
            reason = "dedicated: production provider is not ready"
        else:
            record = observed.releases.get(tag)
            decision = compatibility.compare_live_feed_compatibility(
                None if record is None else record.live_feed_requirements,
                None if production_instance is None else production_instance.evidence,
            )
            reason = f"dedicated: {decision.reason}"
            if decision.compatible and production_instance is not None:
                evidence = production_instance.evidence
                if (
                    evidence["release_tag"] != production_tag
                    or evidence["launch_instance_id"] != production_instance.instance_id
                    or evidence["startup_publication"] != {
                        "path": production_instance.manifest, "sha256": production_instance.manifest_sha256,
                    }
                ):
                    decision = compatibility.CompatibilityDecision(False, "production instance identity differs")
                    reason = f"dedicated: {decision.reason}"
            if decision.compatible:
                provider_tag = production_tag
                reason = f"shared with {production_tag}"
        bindings[tag] = _live_feed_binding(observed, tag, provider_tag, reason)
    return bindings


def validate_live_feed_bindings(
    desired: DesiredReleases, observed: ObservedState,
    proposed: dict[str, ResolvedLiveFeedBinding],
) -> None:
    """Require the same complete decision and ready providers before activation."""
    current = resolve_live_feed_bindings(desired, observed)
    if current != proposed:
        raise ReleaseConfigError("live-feed binding evidence changed before activation")
    for tag, binding in current.items():
        if not dedicated_live_feed_ready(observed, binding.provider_tag):
            raise ReleaseConfigError(f"live-feed provider for {tag} is not ready")
        if binding.shared and binding.provider_tag != desired.production.tag:
            raise ReleaseConfigError(f"live-feed provider for {tag} is not production")


def _binding_changed(observed: ObservedState, binding: ResolvedLiveFeedBinding) -> bool:
    record = observed.releases.get(binding.release_tag)
    if record is None:
        return True
    if binding.provider is None and record.live_feed_provider is None:
        return False
    return record.live_feed_provider != binding.provider or record.live_feed_reason != binding.reason


def _desired_sunset_tags(desired: DesiredReleases) -> list[str]:
    return [binding.tag for binding in desired.sunset]


def plan_reconciliation(
    desired: DesiredReleases,
    observed: ObservedState,
    *,
    force_production_tag: str | None = None,
) -> ReconciliationPlan:
    production_tags = [desired.production.tag, *_desired_sunset_tags(desired)]
    bindings = resolve_live_feed_bindings(desired, observed)
    for tag in production_tags:
        record = observed.releases.get(tag)
        if record is None or record.build_status != "passed":
            return ReconciliationPlan([ReconcileAction("build_release", tag)])
        provider_tag = bindings[tag].provider_tag
        if not dedicated_live_feed_ready(observed, provider_tag):
            return ReconciliationPlan([ReconcileAction("start_live_feeds", provider_tag)])

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

    desired_sunset = _desired_sunset_tags(desired)
    if observed.sunset != desired_sunset:
        return ReconciliationPlan([ReconcileAction("activate_generation")])

    if any(_binding_changed(observed, bindings[tag]) for tag in production_tags):
        return ReconciliationPlan([ReconcileAction("activate_generation")])

    # Serving an already-promoted release is not staging qualification. Refresh
    # its deployment evidence (and retained clients' evidence) before spending
    # time on a new staging build. Legacy observed records default to pending.
    for tag in production_tags:
        if observed.releases[tag].deployment_status != "passed":
            return ReconciliationPlan([ReconcileAction("check_deployment", tag)])

    staging_tag = desired.staging.tag if desired.staging is not None else None
    if staging_tag is not None:
        staging_record = observed.releases.get(staging_tag)
        if staging_record is None or staging_record.build_status != "passed":
            return ReconciliationPlan([ReconcileAction("build_release", staging_tag)])
        if not dedicated_live_feed_ready(observed, staging_tag):
            return ReconciliationPlan([ReconcileAction("start_live_feeds", staging_tag)])
        if observed.staging != staging_tag or _binding_changed(observed, bindings[staging_tag]):
            return ReconciliationPlan([ReconcileAction("activate_generation", staging_tag)])
        if staging_record.qualification_status != "passed":
            return ReconciliationPlan([ReconcileAction("qualify_release", staging_tag)])
        if staging_record.deployment_status != "passed":
            return ReconciliationPlan([ReconcileAction("check_deployment", staging_tag)])

    if observed.staging != staging_tag or observed.sunset != desired_sunset:
        return ReconciliationPlan([ReconcileAction("activate_generation")])
    return ReconciliationPlan([])


def deployment_checks_are_only_pending_work(
    desired: DesiredReleases, observed: ObservedState
) -> bool:
    """Look beyond the first check action without changing real observations."""
    checked = replace(
        observed,
        releases={
            tag: replace(record, deployment_status="passed")
            for tag, record in observed.releases.items()
        },
    )
    return plan_reconciliation(desired, checked).converged


@dataclass(frozen=True)
class ChannelManifest:
    release_tag: str
    source_path: Path
    document: dict[str, Any]
    publication_roots: tuple[str, ...]


def discovery_manifests(manifests: Iterable[ChannelManifest]) -> list[ChannelManifest]:
    """Select one publication per exact contract set, preferring input order.

    The controlling release is first. This applies ONLY to shared discovery;
    release-scoped views, live feeds and GC roots must retain every release.
    """
    selected = []
    seen = set()
    for manifest in manifests:
        contracts = manifest.document.get("contracts")
        if not isinstance(contracts, dict) or not contracts or any(
            not isinstance(key, str) or not key.strip()
            or not isinstance(value, str) or not value.strip()
            for key, value in contracts.items()
        ):
            raise ReleaseConfigError(f"product manifest {manifest.source_path} has invalid contracts")
        identity = tuple(sorted(contracts.items()))
        if identity not in seen:
            selected.append(manifest)
            seen.add(identity)
    return selected


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
            _write_json(current_path, [manifest.document for manifest in discovery_manifests(manifests)])
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


def owned_path(build_root: Path, relative: Path | str) -> Path:
    """Resolve an exact controller-owned path without following symlinks.

    In particular, cleanup must never traverse a substituted namespace parent
    or turn an unexpected release-root symlink into an external deletion.
    """
    relative = Path(relative)
    if build_root.is_symlink() or relative.is_absolute() or not relative.parts or ".." in relative.parts:
        raise ReleaseConfigError(f"unsafe controller-owned path: {relative}")
    path = build_root
    for part in relative.parts:
        path = path / part
        if path.is_symlink():
            raise ReleaseConfigError(f"symlink in controller-owned path: {path}")
    return path


def current_generation(build_root: Path) -> Path | None:
    link = build_root / "channel-current"
    if not link.is_symlink():
        if link.exists():
            raise ReleaseConfigError(f"channel-current is not a symlink: {link}")
        return None
    target = link.resolve()
    if target.parent != build_root / "channel-generations":
        raise ReleaseConfigError(f"unsafe channel-current target: {target}")
    owned_path(build_root, target.relative_to(build_root))
    if not target.is_dir():
        raise ReleaseConfigError(f"missing active generation: {target}")
    return target


def _retire_generation(generation: Path, until: datetime) -> None:
    marker = owned_path(generation, GENERATION_RETIREMENT_FILE)
    _write_json_atomic(marker, {
        "schema_version": 1,
        "until_utc": until.isoformat().replace("+00:00", "Z"),
    })


@dataclass(frozen=True)
class GenerationRetention:
    retained: tuple[Path, ...]
    expired: tuple[Path, ...]
    gc_paths: tuple[str, ...]
    release_tags: frozenset[str]


def _launch_gc_path(build_root: Path, value: str) -> str:
    if not isinstance(value, str):
        raise ReleaseConfigError("invalid live-feed launch GC root path")
    relative = Path(value)
    parts = relative.parts
    if (
        relative.as_posix() != value or relative.is_absolute() or len(parts) != 4
        or parts[0] != "live-feed-launches" or parts[2:] != ("packages", "current_artifacts.json")
        or any(part in {".", ".."} for part in parts)
        or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", parts[1])
    ):
        raise ReleaseConfigError(f"invalid live-feed launch GC root path: {value}")
    path = owned_path(build_root, relative)
    if not path.is_file():
        raise ReleaseConfigError(f"missing live-feed launch GC root: {path}")
    return value


def generation_retention(
    build_root: Path, *, now: datetime | None = None,
    draining_until: dict[str, datetime] | None = None,
    pinned_gc_paths: Iterable[str] = (),
) -> GenerationRetention:
    """Plan bounded generation retention; migrate old GC roots conservatively.

    Call under the release-reconciler lock. A legacy rooted predecessor gets
    one persisted grace period on first observation, never a sliding deadline.
    Unrooted historical/abandoned generations need no additional grace.
    """
    now = now or datetime.now(timezone.utc)
    pinned = {_launch_gc_path(build_root, value) for value in pinned_gc_paths}
    active = current_generation(build_root)
    if active is None:
        return GenerationRetention((), (), tuple(sorted(pinned)), frozenset())
    registry = owned_path(build_root, RELEASE_GC_ROOTS)
    rooted = set()
    if registry.exists():
        document = _object(json.loads(registry.read_text()), "release GC roots", {
            "schema_version", "current_artifacts_paths",
        })
        values = document.get("current_artifacts_paths")
        if document.get("schema_version") != 1 or not isinstance(values, list) or not values:
            raise ReleaseConfigError("invalid release GC roots")
        for value in values:
            if not isinstance(value, str):
                raise ReleaseConfigError("invalid release GC root path")
            parts = Path(value).parts
            if parts and parts[0] == "live-feed-launches":
                _launch_gc_path(build_root, value)
                continue
            if len(parts) < 3 or parts[0] != "channel-generations" or ".." in parts or parts[-1] != "current_artifacts.json":
                raise ReleaseConfigError(f"invalid release GC root path: {value}")
            owned_path(build_root, Path(*parts[:2]))
            path = build_root / value
            # Legacy bootstrap intentionally links production/packages to the
            # shared published tree. Read it, but never follow it for deletion.
            if not path.resolve().is_relative_to(build_root):
                raise ReleaseConfigError(f"release GC root escapes build root: {path}")
            if not path.is_file():
                raise ReleaseConfigError(f"missing release GC root: {path}")
            rooted.add(build_root / parts[0] / parts[1])
    retained, expired = [], []
    for generation in sorted((build_root / "channel-generations").iterdir()):
        owned_path(build_root, generation.relative_to(build_root))
        if not generation.is_dir():
            continue
        marker = owned_path(generation, GENERATION_RETIREMENT_FILE)
        if generation == active:
            retained.append(generation)
            continue
        if not marker.exists() and generation in rooted:
            _retire_generation(generation, now + RELEASE_DRAIN_GRACE)
        until = None
        if marker.exists():
            document = _object(json.loads(marker.read_text()), "generation retirement", {
                "schema_version", "until_utc",
            })
            if document.get("schema_version") != 1:
                raise ReleaseConfigError(f"invalid generation retirement: {marker}")
            value = _parse_utc(document.get("until_utc"), str(marker))
            until = datetime.fromisoformat(value.replace("Z", "+00:00"))
        if generation in rooted and draining_until:
            # A recovered service drain can finish later than its generation's
            # original lease. Keep its publication rooted until both finish.
            for path in _generation_gc_paths(build_root, generation):
                parts = Path(path).parts
                if len(parts) >= 4 and parts[2] == "releases":
                    deadline = draining_until.get(parts[3])
                    if deadline is not None and deadline > now and (until is None or deadline > until):
                        until = deadline
                        _retire_generation(generation, until)
        (retained if until is not None and until > now else expired).append(generation)
    paths = sorted(pinned | {path for generation in retained for path in _generation_gc_paths(build_root, generation)})
    tags = set()
    for value in paths:
        parts = Path(value).parts
        if len(parts) >= 4 and parts[2] == "releases":
            tags.add(_release_tag(parts[3], "generation release"))
    return GenerationRetention(tuple(retained), tuple(expired), tuple(paths), frozenset(tags))


def write_generation_gc_roots(build_root: Path, paths: Iterable[str]) -> None:
    _write_json_atomic(owned_path(build_root, RELEASE_GC_ROOTS), {
        "schema_version": CHANNEL_GENERATION_SCHEMA_VERSION,
        "current_artifacts_paths": sorted(set(paths)),
    })


def activate_channel_generation(
    build_root: Path, generation_root: Path, *, now: datetime | None = None,
    pinned_gc_paths: Iterable[str] = (),
) -> None:
    """Atomically direct new requests at a complete generation.

    GC is rooted first and includes every still-draining generation, including
    rapid successive activations. Persist leases before changing roots/pointers
    so interruption or rollback cannot discard either side of a channel switch.
    """

    build_root = build_root.resolve()
    generation_root = owned_path(build_root, generation_root.relative_to(build_root))
    if generation_root.parent != build_root / "channel-generations":
        raise ReleaseConfigError(f"unsafe generation target: {generation_root}")
    new_roots = _generation_gc_paths(build_root, generation_root)
    now = now or datetime.now(timezone.utc)
    retention = generation_retention(build_root, now=now, pinned_gc_paths=pinned_gc_paths)
    current_link = build_root / "channel-current"
    previous_generation = current_generation(build_root)
    if previous_generation is not None and previous_generation != generation_root:
        _retire_generation(previous_generation, now + RELEASE_DRAIN_GRACE)
    # Also protect a candidate if activation fails before replacing the pointer.
    _retire_generation(generation_root, now + RELEASE_DRAIN_GRACE)
    write_generation_gc_roots(build_root, [*new_roots, *retention.gc_paths])

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
