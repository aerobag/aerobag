#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Fixture-free controller/real-merger regression, invoked by Cargo integration CI."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import reconcile_prod_releases as controller  # noqa: E402
import release_reconciler as releases  # noqa: E402


def main() -> None:
    merger = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix="aerobag-promotion-merge-") as temp:
        root = Path(temp)
        instance = controller.Controller.__new__(controller.Controller)
        instance.args = SimpleNamespace(
            observed=root / "observed.json", force_production_tag=None,
            controller_preprocessor=merger,
        )
        instance.artifact_root = root
        instance.desired = releases.DesiredReleases(
            production=releases.ReleaseBinding("new"), staging=None,
            sunset=(releases.SunsetBinding("old", "2099-01-01T00:00:00Z"),
                    releases.SunsetBinding("legacy", "2099-01-01T00:00:00Z")),
        )
        instance.observed = releases.ObservedState(production="old", staging="new")
        for index, (tag, contract) in enumerate((("new", "NAV25"), ("old", "NAV25"), ("legacy", "NAV23"))):
            publication = root / "published" / tag
            (publication / "packaged").mkdir(parents=True)
            (publication / "unpacked").mkdir()
            manifest = publication / "product_artifacts.json"
            manifest.write_text(json.dumps({
                "schema_version": 1, "contracts": {"nav-db": contract},
                "artifact_roots": {"packaged": f"{tag}/packaged/", "unpacked": f"{tag}/unpacked/"},
                "as_of_date": "2026-09-11", "as_of_utc": "2026-09-11T00:00:00Z", "bundles": [],
            }))
            release_root = root / "release-builds" / tag
            (release_root / "web").mkdir(parents=True)
            (release_root / "downloads").mkdir()
            (release_root / "web/index.html").write_text(tag)
            (release_root / "downloads/app.apk").write_bytes(tag.encode())
            instance.observed.releases[tag] = releases.ObservedRelease(
                tag=tag, tag_object="a" * 40, commit="b" * 40,
                build_status="passed", qualification_status="passed",
                release_root=str(release_root), product_manifest=str(manifest),
                live_feed_endpoint=f"http://127.0.0.1:{8100 + index}", live_feed_status="running",
            )

        merges = []

        # The Rust contract remains strict: passing both same-contract releases
        # without controller selection must still be rejected.
        raw = subprocess.run([
            str(merger), "merge-current-artifacts", "--build-root", str(root),
            "--output", str(root / "unselected/current_artifacts.json"),
            "--manifest", instance.observed.releases["new"].product_manifest,
            "--manifest", instance.observed.releases["old"].product_manifest,
        ], capture_output=True, text=True, timeout=20)
        assert raw.returncode != 0 and "duplicate contract set" in raw.stderr, raw.stderr

        def run(command, **_kwargs):
            if command[0] == str(merger):
                result = subprocess.run(command, capture_output=True, text=True, timeout=20)
                assert result.returncode == 0, result.stdout + result.stderr
                merges.append(command)
            else:
                # No system services or network: only the merge crosses into Rust.
                assert command in (["nginx", "-t"], ["systemctl", "reload", "nginx.service"]), command

        with (
            mock.patch.object(controller, "_run", side_effect=run),
            mock.patch.object(controller.release_builder, "normalize_release_permissions"),
            mock.patch.object(controller.release_builder, "validate_release_directory"),
            mock.patch.object(instance, "validate_public_production"),
        ):
            instance.activate()

        current = root / "channel-current"
        combined = json.loads((current / "production/packages/current_artifacts.json").read_text())
        assert [value["artifact_roots"]["packaged"] for value in combined] == ["new/packaged/", "legacy/packaged/"]
        assert instance.observed.production == "new" and instance.observed.staging is None
        assert (current / "production/web/index.html").read_text() == "new"
        assert (current / "production/downloads/app.apk").read_bytes() == b"new"
        assert len(merges) == 1
        gc = json.loads((root / "state/release-gc-roots.json").read_text())["current_artifacts_paths"]
        routes = json.loads((current / "live-feed-routes.json").read_text())
        for tag in ("new", "old", "legacy"):
            scoped = current / "releases" / tag
            discovery = json.loads((scoped / "packages/current_artifacts.json").read_text())
            assert discovery[0]["artifact_roots"]["packaged"] == f"{tag}/packaged/"
            assert (scoped / "packages" / tag).resolve() == root / "published" / tag
            assert (scoped / "web/index.html").read_text() == tag
            assert (scoped / "downloads/app.apk").read_bytes() == tag.encode()
            assert tag in routes["releases"]
            assert any(path.endswith(f"releases/{tag}/packages/current_artifacts.json") for path in gc)
        print("PASS: real merge, production preference, sunset endpoints and GC roots")


if __name__ == "__main__":
    main()
