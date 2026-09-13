# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Bounded retirement of immutable live-feed instances, never release assets."""

from __future__ import annotations

import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

import live_feed_launch as launch
import release_reconciler as releases
import release_retirement as retirement


def instance_paths(root: Path, instance: releases.LiveFeedInstance, env_root: Path) -> tuple[list[Path], Path]:
    tag = releases._release_tag(instance.release_tag, "live-feed instance release")
    if (not isinstance(instance.launch_digest, str)
            or not re.fullmatch(r"[0-9a-f]{64}", instance.launch_digest)
            or instance.instance_id != f"{tag}-{instance.launch_digest[:16]}"):
        raise releases.ReleaseConfigError("invalid immutable live-feed instance identity")
    if instance.unit != f"aerobag-live-feeds-release@{instance.instance_id}.service":
        raise releases.ReleaseConfigError(f"unexpected live-feed instance unit: {instance.unit}")
    launch_root = launch.instance_root(root, instance.instance_id)
    paths = [releases.owned_path(root, f"{prefix}/{instance.instance_id}") for prefix in (
        "live-feeds/instances", "scratch/live-feeds/instances", "state/live-feeds/instances",
    )] + [launch_root]
    if sorted(instance.roots) != sorted(str(path) for path in paths):
        raise releases.ReleaseConfigError(f"unexpected live-feed instance roots: {instance.instance_id}")
    if instance.manifest != str(launch_root / "packages/product_artifacts.json"):
        raise releases.ReleaseConfigError(f"unexpected live-feed startup manifest: {instance.instance_id}")
    if instance.gc_paths != [launch.launch_gc_path(root, instance.instance_id)]:
        raise releases.ReleaseConfigError(f"unexpected live-feed GC roots: {instance.instance_id}")
    environment = releases.owned_path(env_root, f"{instance.instance_id}.env")
    if environment.exists() and not environment.is_file():
        raise releases.ReleaseConfigError(f"unexpected live-feed environment: {environment}")
    return paths, environment


def retire_instances(
    root: Path, observed: releases.ObservedState, *,
    active_provider_ids: set[str], candidate_instance_ids: set[str], now: datetime,
    save: Callable[[], None], release_gc_roots: Callable[[set[str]], None],
    env_root: Path = Path("/etc/aerobag/live-feeds"), run: Callable | None = None,
) -> set[str]:
    """Run under the controller lock; active IDs include retained serving bindings.

    Unbound services stop immediately. Their non-sliding drain deadline protects
    files and pins only; stopped candidates retain these resources indefinitely.
    release_gc_roots must persistently exclude the supplied IDs before returning,
    preserving every other generation/instance pin. It must not run publication
    GC. Removed descriptors remain loadable and file cleanup is retryable.
    """
    if now.tzinfo is None or now.utcoffset() is None:
        raise releases.ReleaseConfigError("instance retirement requires a timezone-aware clock")
    run = run or subprocess.run
    instances = observed.live_feed_instances
    missing = (active_provider_ids | candidate_instance_ids) - instances.keys()
    if missing:
        raise releases.ReleaseConfigError(f"missing protected live-feed instances: {sorted(missing)}")
    validated = {}
    for key, instance in instances.items():
        if key != instance.instance_id:
            raise releases.ReleaseConfigError(f"live-feed instance key disagrees with identity: {key}")
        if instance.status not in {"pending", "running", "unavailable", "failed", "stopped", "removed"}:
            raise releases.ReleaseConfigError(f"unknown live-feed instance status: {instance.status}")
        if key in active_provider_ids and instance.status == "removed":
            raise releases.ReleaseConfigError(f"removed live-feed instance is still required: {key}")
        if instance.draining_until_utc is not None:
            releases._parse_utc(instance.draining_until_utc, f"instance {key} drain deadline")
        validated[key] = instance_paths(root, instance, env_root)

    def service_state(instance: releases.LiveFeedInstance) -> str:
        result = run(["systemctl", "show", instance.unit, "--property=ActiveState", "--value"],
                     check=True, text=True, stdout=subprocess.PIPE, timeout=30)
        state = result.stdout.strip()
        if state not in {"active", "activating", "reloading", "deactivating", "inactive", "failed"}:
            raise RuntimeError(f"unknown systemd state for {instance.unit}: {state!r}")
        return state

    obsolete = {}
    for key, instance in sorted(instances.items()):
        if key in active_provider_ids:
            if instance.draining_until_utc is not None:
                instance.draining_until_utc = None
                save()
            continue
        paths, environment = validated[key]
        if instance.status == "removed" and not any(path.exists() for path in [*paths, environment]):
            continue
        deadline = (datetime.fromisoformat(instance.draining_until_utc.replace("Z", "+00:00"))
                    if instance.draining_until_utc is not None else None)
        state = service_state(instance)
        if instance.status == "removed":
            if state not in {"inactive", "failed"}:
                raise RuntimeError(f"removed live-feed instance is unexpectedly active: {instance.unit}")
            obsolete[key] = (instance, paths, environment)
            continue
        if deadline is None:
            deadline = now + releases.RELEASE_DRAIN_GRACE
            instance.draining_until_utc = deadline.astimezone(
                timezone.utc).isoformat().replace("+00:00", "Z")
            save()
        if instance.status != "stopped" or state not in {"inactive", "failed"}:
            run(["systemctl", "disable", "--now", instance.unit], check=True, timeout=30)
            if service_state(instance) not in {"inactive", "failed"}:
                raise RuntimeError(f"live-feed instance did not stop: {instance.unit}")
        if instance.status != "stopped" or instance.evidence is not None:
            instance.status = "stopped"
            instance.evidence = None
            save()
        if key not in candidate_instance_ids and deadline <= now:
            obsolete[key] = (instance, paths, environment)
    if not obsolete:
        return set()

    observed.gc_pending = True
    save()
    release_gc_roots(set(obsolete))
    for instance, _paths, _environment in obsolete.values():
        instance.status = "removed"
    save()
    for instance, paths, environment in obsolete.values():
        # Recheck immediately before deletion, including on interrupted retries.
        instance_paths(root, instance, env_root)
        for path in paths:
            retirement.remove_owned_path(root, path)
        retirement.remove_owned_path(env_root, environment)
    return set(obsolete)
