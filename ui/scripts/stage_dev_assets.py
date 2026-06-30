#!/usr/bin/env python3
from __future__ import annotations

import json
import os
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TARGET_ROOT_FILE = ROOT / "ui" / "target-root.txt"
ARTIFACT_READ_PATH_CONFIG = ROOT / ".aerobag-artifact-read-path"
WEB_STATIC_ROOT: Path


def resolve_ui_target_root() -> Path:
    env_value = os.environ.get("AEROBAG_UI_TARGET_ROOT")
    if env_value:
        return Path(env_value).expanduser()
    return (ROOT / TARGET_ROOT_FILE.read_text().strip()).resolve()


def resolve_artifact_root() -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_READ_PATH")
    if env_value:
        candidate = Path(env_value).expanduser()
    else:
        configured = ARTIFACT_READ_PATH_CONFIG.read_text().strip()
        candidate = Path(configured)
        if not candidate.is_absolute():
            candidate = (ROOT / candidate).resolve()
    current = candidate / "current_artifacts.json"
    if not current.exists():
        return candidate
    if not current.is_file():
        raise RuntimeError(f"artifact root current_artifacts is not a file: {current}")
    current_artifacts = json.loads(current.read_text())
    if not isinstance(current_artifacts, list) or not current_artifacts:
        raise RuntimeError(f"{current} must be a non-empty current_artifacts list")
    for index, manifest in enumerate(current_artifacts):
        artifact_roots = manifest.get("artifact_roots") if isinstance(manifest, dict) else None
        if not isinstance(artifact_roots, dict):
            raise RuntimeError(
                f"{current}[{index}] must have artifact_roots with packaged and unpacked paths"
            )
        for key in ("packaged", "unpacked"):
            root = artifact_roots.get(key)
            if not isinstance(root, str) or not root.strip() or root.startswith("/") or ".." in root.split("/"):
                raise RuntimeError(f"{current}[{index}].artifact_roots.{key} is not a safe relative path: {root!r}")
    return candidate


ARTIFACT_ROOT = resolve_artifact_root()
UI_TARGET_ROOT = resolve_ui_target_root()
WEB_STATIC_ROOT = UI_TARGET_ROOT / "web" / "generated-static"
STAGE_STAMP_PATH = WEB_STATIC_ROOT / ".stage-stamp.json"


def current_stage_stamp() -> dict:
    current = ARTIFACT_ROOT / "current_artifacts.json"
    try:
        stat = current.stat()
        current_artifacts = {
            "path": str(current),
            "exists": True,
            "size": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
        }
    except FileNotFoundError:
        current_artifacts = {
            "path": str(current),
            "exists": False,
            "size": None,
            "mtime_ns": None,
        }
    return {
        "artifact_root": str(ARTIFACT_ROOT),
        "current_artifacts": current_artifacts,
        "version": 12,
    }


def main() -> None:
    # Artifact data is no longer staged into generated-static. Vite and production
    # servers expose the artifact root as the single /packages/ tree, matching the
    # public publication contract.
    WEB_STATIC_ROOT.mkdir(parents=True, exist_ok=True)
    STAGE_STAMP_PATH.write_text(json.dumps(current_stage_stamp(), indent=2, sort_keys=True) + "\n")
    print(f"Artifact root ready at /packages: {ARTIFACT_ROOT}")


if __name__ == "__main__":
    main()
