#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Exact, release-owned cleanup targets; shared publications remain owned by GC."""

from __future__ import annotations

import shutil
from pathlib import Path

import release_reconciler as releases


def release_paths(root: Path, record: releases.ObservedRelease) -> list[Path]:
    tag = releases._release_tag(record.tag, "retired release")
    if not releases.COMMIT_RE.fullmatch(record.commit):
        raise releases.ReleaseConfigError(f"invalid retired release commit: {tag}")
    binary_root = releases.owned_path(root, f"release-builds/{tag}-{record.commit[:12]}")
    # Do not trust a stale/corrupted observed path as authority to delete data.
    if record.release_root is not None and Path(record.release_root) != binary_root:
        raise releases.ReleaseConfigError(f"unexpected release_root for {tag}: {record.release_root}")
    return [binary_root, *(
        releases.owned_path(root, f"{namespace}/{tag}")
        for namespace in (
            "live-feeds/releases", "scratch/live-feeds/releases",
            "state/live-feeds/releases", "state/deployment-checks",
        )
    ), releases.owned_path(root, f"state/release-build-results/{tag}.json")]


def remove_owned_path(root: Path, path: Path) -> None:
    # Check again immediately before removal. rmtree does not follow symlinks
    # inside the tree; hard links merely lose this release's directory entry.
    releases.owned_path(root, path.relative_to(root))
    if path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink(missing_ok=True)


def forget_removed_artifacts(record: releases.ObservedRelease) -> None:
    """Keep immutable tag/commit identity, but require rebuild if reintroduced."""
    record.build_status = "pending"
    record.release_root = None
    record.product_manifest = None
    record.qualification_status = "pending"
    record.qualification_record = None
    record.deployment_status = "pending"
    record.deployment_record = None
    record.deployment_error = None
    record.live_feed_status = "stopped"
    record.live_feed_endpoint = None
    record.draining_until_utc = None
