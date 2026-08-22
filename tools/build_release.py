#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Build immutable, channel-independent application artifacts for one release."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--cargo-target-dir", type=Path, required=True)
    parser.add_argument("--ui-target-root", type=Path, required=True)
    parser.add_argument("--public-origin", default="https://aerobag.org")
    return parser.parse_args()


def release_directory(artifact_root: Path, tag: str, commit: str) -> Path:
    return artifact_root / "release-builds" / f"{tag}-{commit[:12]}"


def release_environment(
    tag: str,
    *,
    public_origin: str,
    web_dist: Path,
    ui_target_root: Path,
) -> dict[str, str]:
    origin = public_origin.rstrip("/")
    release_url = f"{origin}/releases/{tag}"
    return {
        "AEROBAG_WEB_DIST": str(web_dist),
        "AEROBAG_WEB_PUBLIC_BASE_URL": f"/releases/{tag}/web/",
        "AEROBAG_PACKAGE_SOURCE_BASE_URL": f"{release_url}/packages/",
        "AEROBAG_LIVE_FEEDS_ORIGIN": release_url,
        "AEROBAG_DOWNLOADS_BASE_URL": f"{release_url}/downloads",
        "AEROBAG_UI_TARGET_ROOT": str(ui_target_root),
        "ANDROID_PACKAGE_SOURCE_BASE_URL": f"{release_url}/packages/",
        "ANDROID_LIVE_FEED_SOURCE_BASE_URL": release_url,
        "ANDROID_CLOUD_SERVER_BASE_URL": f"{origin}/cloud/",
        "ANDROID_APK_PUBLIC_BASE_URL": f"/releases/{tag}/downloads",
    }


def _run(command: list[str], *, cwd: Path, env: dict[str, str]) -> None:
    print(f"+ cd {cwd} && {' '.join(command)}", flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def directory_sha256(path: Path) -> str:
    if not path.is_dir():
        raise RuntimeError(f"immutable release directory is missing: {path}")
    digest = hashlib.sha256()
    for member in sorted(path.rglob("*")):
        if member.is_symlink():
            raise RuntimeError(f"immutable release tree must not contain symlinks: {member}")
        if not member.is_file():
            continue
        relative = member.relative_to(path).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(bytes.fromhex(_sha256(member)))
    return digest.hexdigest()


def validate_release_directory(path: Path, tag: str, commit: str) -> dict:
    metadata_path = path / "release.json"
    document = json.loads(metadata_path.read_text(encoding="utf-8"))
    if document.get("tag") != tag or document.get("commit") != commit:
        raise RuntimeError(f"immutable release directory identity mismatch: {path}")
    artifacts = document.get("artifacts")
    if not isinstance(artifacts, dict):
        raise RuntimeError(f"immutable release has no artifact identities: {path}")
    web = artifacts.get("web")
    if not isinstance(web, dict) or web.get("sha256") != directory_sha256(
        path / "web"
    ):
        raise RuntimeError(f"immutable release web artifact mismatch: {path}")
    downloads = artifacts.get("downloads")
    if not isinstance(downloads, dict) or downloads.get(
        "sha256"
    ) != directory_sha256(path / "downloads"):
        raise RuntimeError(f"immutable release downloads artifact mismatch: {path}")
    for key, directory in (
        ("apk", "downloads"),
        ("live_feeds_binary", "bin"),
        ("preprocessor_binary", "bin"),
    ):
        artifact = artifacts.get(key)
        if not isinstance(artifact, dict):
            raise RuntimeError(f"immutable release has no {key} identity: {path}")
        filename = artifact.get("filename")
        if (
            not isinstance(filename, str)
            or not filename
            or Path(filename).name != filename
        ):
            raise RuntimeError(f"immutable release has unsafe {key} filename: {path}")
        member = path / directory / filename
        if artifact.get("sha256") != _sha256(member):
            raise RuntimeError(f"immutable release {key} mismatch: {member}")
    return document


def collect_web_build_output(web_dist: Path, build_ui_root: Path) -> None:
    legacy_web_dist = build_ui_root / "web/dist"
    if not web_dist.is_dir() and legacy_web_dist.is_dir():
        web_dist.parent.mkdir(parents=True, exist_ok=True)
        legacy_web_dist.rename(web_dist)
    if not web_dist.is_dir():
        raise RuntimeError("web build did not produce an isolated release tree")


def _load_existing_release(path: Path, tag: str, commit: str) -> bool:
    metadata_path = path / "release.json"
    if not metadata_path.is_file():
        return False
    validate_release_directory(path, tag, commit)
    return True


def build_release(args: argparse.Namespace) -> Path:
    repo_root = args.repo_root.resolve()
    actual_commit = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()
    if actual_commit != args.commit:
        raise RuntimeError(
            f"release worktree is {actual_commit}, expected configured commit {args.commit}"
        )

    final_root = release_directory(args.artifact_root.resolve(), args.tag, args.commit)
    if final_root.exists():
        if _load_existing_release(final_root, args.tag, args.commit):
            print(final_root)
            return final_root
        raise RuntimeError(f"incomplete immutable release directory already exists: {final_root}")

    final_root.parent.mkdir(parents=True, exist_ok=True)
    temporary_root = Path(
        tempfile.mkdtemp(prefix=f".{final_root.name}.", dir=final_root.parent)
    )
    try:
        web_dist = temporary_root / "web"
        downloads = temporary_root / "downloads"
        binary_dir = temporary_root / "bin"
        build_ui_root = temporary_root / "ui-target"
        shared_ui_root = args.ui_target_root.resolve()
        binary_dir.mkdir()
        env = os.environ.copy()
        env.update(
            release_environment(
                args.tag,
                public_origin=args.public_origin,
                web_dist=web_dist,
                ui_target_root=build_ui_root,
            )
        )
        env["CARGO_TARGET_DIR"] = str(args.cargo_target_dir.resolve())
        env["AEROBAG_UI_RUST_TARGET_DIR"] = str(shared_ui_root / "shared/rust-target")
        env["AEROBAG_WEB_WORKSPACE_DIR"] = str(
            shared_ui_root / "web/release-workspace"
        )
        env["GRADLE_USER_HOME"] = str(shared_ui_root / "android/gradle-user-home")
        env["PROJECT_CACHE_DIR"] = str(shared_ui_root / "android/project-cache")
        env["BINARYEN_INSTALL_ROOT"] = str(shared_ui_root / "tools")
        env["AEROBAG_WASM_OPT_BIN"] = (
            f"node {shared_ui_root / 'tools/binaryen-version_129/wasm-opt.js'}"
        )

        _run(
            [
                "cargo",
                "build",
                "--release",
                "-p",
                "live-feeds-daemon",
                "-p",
                "preprocessor-cli",
            ],
            cwd=repo_root / "product/preprocessor",
            env=env,
        )
        live_binary = args.cargo_target_dir.resolve() / "release/aerobag-live-feedsd"
        shutil.copy2(live_binary, binary_dir / live_binary.name)
        preprocessor_binary = args.cargo_target_dir.resolve() / "release/preprocessor-cli"
        shutil.copy2(preprocessor_binary, binary_dir / preprocessor_binary.name)

        _run(["npm", "run", "install:wasm-opt"], cwd=repo_root / "ui/web-app", env=env)
        _run(["npm", "run", "build:release"], cwd=repo_root / "ui/web-app", env=env)
        collect_web_build_output(web_dist, build_ui_root)
        _run(
            ["./scripts/build_prod_apk.sh"],
            cwd=repo_root / "ui/android-app",
            env=env,
        )
        built_downloads = web_dist / "downloads"
        if not built_downloads.is_dir():
            raise RuntimeError("Android build did not produce release downloads")
        built_downloads.rename(downloads)
        shutil.rmtree(build_ui_root)

        apk_metadata = json.loads(
            (downloads / "android-apk.json").read_text(encoding="utf-8")
        )
        apk_path = downloads / apk_metadata["filename"]
        metadata = {
            "schema_version": 1,
            "tag": args.tag,
            "commit": args.commit,
            "built_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "endpoints": {
                "packages": env["AEROBAG_PACKAGE_SOURCE_BASE_URL"],
                "live_feeds": f"{env['AEROBAG_LIVE_FEEDS_ORIGIN']}/live-feeds/",
                "web": f"{args.public_origin.rstrip('/')}/releases/{args.tag}/web/",
            },
            "artifacts": {
                "web": {
                    "sha256": directory_sha256(web_dist),
                },
                "downloads": {
                    "sha256": directory_sha256(downloads),
                },
                "apk": {
                    "filename": apk_path.name,
                    "sha256": _sha256(apk_path),
                },
                "live_feeds_binary": {
                    "filename": live_binary.name,
                    "sha256": _sha256(binary_dir / live_binary.name),
                },
                "preprocessor_binary": {
                    "filename": preprocessor_binary.name,
                    "sha256": _sha256(binary_dir / preprocessor_binary.name),
                },
            },
        }
        (temporary_root / "release.json").write_text(
            json.dumps(metadata, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        validate_release_directory(temporary_root, args.tag, args.commit)
        os.replace(temporary_root, final_root)
    except BaseException:
        shutil.rmtree(temporary_root, ignore_errors=True)
        raise
    print(final_root)
    return final_root


def main() -> int:
    build_release(parse_args())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
