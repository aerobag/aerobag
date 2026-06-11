#!/usr/bin/env python3
"""Build the production web tree and publish the versioned Android APK."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_ui_target_root(root: Path) -> Path:
    configured = (root / "ui" / "target-root.txt").read_text(encoding="utf-8").strip()
    path = Path(configured)
    if path.is_absolute():
        return path
    return (root / path).resolve()


def resolve_env(root: Path) -> dict[str, str]:
    env = os.environ.copy()
    env.setdefault("SOURCE_ROOT", str(root))
    env.setdefault("AEROBAG_UI_TARGET_ROOT", str(default_ui_target_root(root)))
    env.setdefault("AEROBAG_WEB_DIST", str(Path(env["AEROBAG_UI_TARGET_ROOT"]) / "web" / "dist"))
    return env


def run(command: list[str], *, cwd: Path, env: dict[str, str], dry_run: bool) -> None:
    printable = " ".join(command)
    print(f"+ cd {cwd}")
    print(f"+ {printable}")
    if dry_run:
        return
    subprocess.run(command, cwd=cwd, env=env, check=True)


def build_product(root: Path, env: dict[str, str], dry_run: bool) -> None:
    preproc_dir = root / "product" / "preprocessor"
    run(
        ["cargo", "build", "--release", "-p", "preprocessor-cli", "-p", "live-feeds-daemon"],
        cwd=preproc_dir,
        env=env,
        dry_run=dry_run,
    )
    cargo_target_dir = env.get("CARGO_TARGET_DIR")
    binary = Path(cargo_target_dir) / "release" / "preprocessor-cli" if cargo_target_dir else preproc_dir / "target" / "release" / "preprocessor-cli"
    run(
        [str(binary), "build-product", "--source-root", str(root)],
        cwd=preproc_dir,
        env=env,
        dry_run=dry_run,
    )


def build_web(root: Path, env: dict[str, str], dry_run: bool) -> None:
    run(["npm", "run", "build:release"], cwd=root / "ui" / "web-app", env=env, dry_run=dry_run)


def publish_android(root: Path, env: dict[str, str], dry_run: bool) -> None:
    run([str(root / "ui" / "android-app" / "scripts" / "build_prod_apk.sh")], cwd=root / "ui" / "android-app", env=env, dry_run=dry_run)


def verify_android_metadata(env: dict[str, str]) -> None:
    metadata = Path(env["AEROBAG_WEB_DIST"]) / "downloads" / "android-apk.json"
    if not metadata.is_file():
        raise FileNotFoundError(f"missing Android APK metadata: {metadata}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--skip-product",
        action="store_true",
        help="skip the preprocessor product build and reuse the existing published artifact set",
    )
    parser.add_argument("--skip-web", action="store_true", help="do not build the web static tree")
    parser.add_argument("--skip-android", action="store_true", help="do not publish the Android APK")
    parser.add_argument("--dry-run", action="store_true", help="print commands without running them")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    root = repo_root()
    env = resolve_env(root)

    if not args.skip_product:
        build_product(root, env, args.dry_run)
    if not args.skip_web:
        build_web(root, env, args.dry_run)
    if not args.skip_android:
        publish_android(root, env, args.dry_run)
        if not args.dry_run:
            verify_android_metadata(env)

    print(f"deploy tree: {env['AEROBAG_WEB_DIST']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
