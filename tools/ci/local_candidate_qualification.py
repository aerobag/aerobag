#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Run the complete release-candidate workload locally and record its result."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_REPETITIONS = 5
ANDROID_SHARDS = 4
DEFAULT_ANDROID_WORKERS = 2
NEXTEST_VERSION = "0.9.140"
PRIORITIES = ("p0", "p1", "p2")
NATIVE_TESTS = (
    "android.flight-plan-route-smoke",
    "android.plate-first-render-smoke",
    "android.map-follow-ctr-gesture-smoke",
    "android.layer-toggle-navdb-regression",
    "android.rotation-session-retention-regression",
)
CI_SIGNING_KEY = ROOT / ".ci/android-ci.keystore"
CI_SIGNING_ENVIRONMENT = {
    "AEROBAG_ANDROID_KEYSTORE": str(CI_SIGNING_KEY),
    "AEROBAG_ANDROID_KEYSTORE_PASSWORD": "android",
    "AEROBAG_ANDROID_KEY_ALIAS": "androiddebugkey",
    "AEROBAG_ANDROID_KEY_PASSWORD": "android",
}


class QualificationError(RuntimeError):
    pass


@dataclass(frozen=True)
class Lane:
    name: str
    command: tuple[str, ...]
    cwd: Path = ROOT
    env: dict[str, str] | None = None
    timeout_seconds: int = 7_200


@dataclass(frozen=True)
class LaneResult:
    name: str
    returncode: int
    duration_seconds: float
    log_path: Path

    @property
    def passed(self) -> bool:
        return self.returncode == 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the exact release-candidate workload before pushing it to GitHub."
    )
    parser.add_argument("--repetitions", type=int, default=DEFAULT_REPETITIONS)
    parser.add_argument("--jobs", type=int, default=min(8, os.cpu_count() or 1))
    parser.add_argument(
        "--android-workers",
        type=int,
        default=DEFAULT_ANDROID_WORKERS,
        help=(
            "number of Android emulators sharing this host; one models each GitHub matrix "
            "runner, while larger values are an optional host-contention stress mode"
        ),
    )
    parser.add_argument("--check", action="store_true", help="only verify the receipt for HEAD")
    return parser.parse_args()


def git(*args: str) -> str:
    return subprocess.check_output(
        ["git", *args], cwd=ROOT, text=True, stderr=subprocess.STDOUT
    ).strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def receipt_path(commit: str) -> Path:
    git_dir = Path(git("rev-parse", "--git-common-dir"))
    if not git_dir.is_absolute():
        git_dir = ROOT / git_dir
    return git_dir.resolve() / "aerobag-local-qualification" / f"{commit}.json"


def workflow_identity() -> dict[str, str]:
    paths = (
        ROOT / ".github/workflows/ci.yml",
        ROOT / ".github/workflows/e2e-ci.yml",
        ROOT / "tools/ci/local_candidate_qualification.py",
        ROOT / "tools/e2e/release_journey_lab.sh",
    )
    return {str(path.relative_to(ROOT)): sha256(path) for path in paths}


def valid_receipt(commit: str) -> dict[str, object] | None:
    path = receipt_path(commit)
    if not path.is_file():
        return None
    try:
        receipt = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    if receipt.get("commit") != commit or receipt.get("status") != "passed":
        return None
    if receipt.get("workflow_identity") != workflow_identity():
        return None
    repetitions = receipt.get("repetitions")
    if not isinstance(repetitions, int) or repetitions < DEFAULT_REPETITIONS:
        return None
    return receipt


def assert_clean_commit() -> str:
    if git("status", "--porcelain"):
        raise QualificationError("local candidate qualification requires a clean checkout")
    commit = git("rev-parse", "HEAD")
    branch = git("branch", "--show-current")
    if branch != "main":
        raise QualificationError(f"local candidate qualification requires main, not {branch}")
    return commit


def lane_environment(extra: dict[str, str] | None = None) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "RUST_TOOLCHAIN": "1.94.1",
            "RUSTUP_TOOLCHAIN": "1.94.1",
            "CARGO_TERM_COLOR": "always",
        }
    )
    if extra:
        env.update(extra)
    return env


def prepare_environment() -> None:
    toolchains = subprocess.check_output(
        ["rustup", "toolchain", "list"], cwd=ROOT, text=True
    )
    if "1.94.1-x86_64-unknown-linux-gnu" not in toolchains.split():
        raise QualificationError("Rust toolchain 1.94.1 is not installed")
    installed_targets = set(
        subprocess.check_output(
            ["rustup", "target", "list", "--toolchain", "1.94.1", "--installed"],
            cwd=ROOT,
            text=True,
        ).split()
    )
    required_targets = {"wasm32-unknown-unknown", "x86_64-linux-android"}
    missing_targets = sorted(required_targets - installed_targets)
    if missing_targets:
        raise QualificationError(
            "Rust 1.94.1 is missing targets: " + ", ".join(missing_targets)
        )
    nextest_version = subprocess.check_output(
        ["cargo", "nextest", "--version"], cwd=ROOT, text=True
    ).splitlines()[0]
    if NEXTEST_VERSION not in nextest_version:
        raise QualificationError(
            f"cargo-nextest {NEXTEST_VERSION} is required, got {nextest_version}"
        )
    node_major = subprocess.check_output(
        ["node", "--version"], cwd=ROOT, text=True
    ).strip().removeprefix("v").split(".", 1)[0]
    if node_major != "20":
        raise QualificationError(f"Node 20 is required, got major version {node_major}")
    subprocess.run(["npm", "ci", "--prefix", "ui/web-app"], cwd=ROOT, check=True)
    subprocess.run(
        [str(ROOT / "tools/ci/install_wasm_bindgen.sh")], cwd=ROOT, check=True
    )


def gradle_wrapper_distribution() -> tuple[str, str]:
    properties = (
        ROOT / "ui/android-app/gradle/wrapper/gradle-wrapper.properties"
    ).read_text(encoding="utf-8")
    distribution_url = next(
        (
            line.split("=", 1)[1].strip()
            for line in properties.splitlines()
            if line.startswith("distributionUrl=")
        ),
        None,
    )
    if not distribution_url:
        raise QualificationError("Gradle wrapper has no distributionUrl")
    archive = distribution_url.rsplit("/", 1)[-1]
    if not archive.endswith(".zip"):
        raise QualificationError(f"unsupported Gradle wrapper distribution {archive}")
    distribution = archive.removesuffix(".zip")
    unpacked = distribution.removesuffix("-bin").removesuffix("-all")
    return distribution, unpacked


def prepare_gradle_wrapper_cache(
    run_root: Path,
    cache_root: Path | None = None,
) -> Path:
    configured_cache = os.environ.get("AEROBAG_GRADLE_WRAPPER_CACHE")
    if cache_root is not None:
        source = cache_root
    elif configured_cache:
        source = Path(configured_cache)
    else:
        source = Path.home() / ".gradle/wrapper"
    source = source.expanduser().resolve()
    distribution, unpacked = gradle_wrapper_distribution()
    candidates = source.glob(f"dists/{distribution}/*")
    ready = any(
        (candidate / f"{distribution}.zip.ok").is_file()
        and (candidate / unpacked).is_dir()
        for candidate in candidates
    )
    if not ready:
        raise QualificationError(
            f"local Gradle wrapper cache {source} lacks {distribution}; "
            "prime it once with GRADLE_USER_HOME=$HOME/.gradle "
            "ui/android-app/gradlew --version"
        )

    for target_root in ("ci-ui-target", "release-ui-target"):
        target = run_root / target_root / "android/gradle-user-home/wrapper"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.symlink_to(source, target_is_directory=True)
    return source


def run_lane(lane: Lane, log_dir: Path) -> LaneResult:
    log_path = log_dir / f"{lane.name}.log"
    log_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    print(f"START {lane.name}", flush=True)
    with log_path.open("w", encoding="utf-8") as log:
        log.write(f"cwd={lane.cwd}\ncommand={json.dumps(lane.command)}\n\n")
        log.flush()
        try:
            result = subprocess.run(
                lane.command,
                cwd=lane.cwd,
                env=lane_environment(lane.env),
                stdout=log,
                stderr=subprocess.STDOUT,
                text=True,
                timeout=lane.timeout_seconds,
                check=False,
            )
            returncode = result.returncode
        except subprocess.TimeoutExpired:
            log.write(f"\nTIMED OUT after {lane.timeout_seconds}s\n")
            returncode = 124
    duration = time.monotonic() - started
    state = "PASS" if returncode == 0 else "FAIL"
    print(f"{state} {lane.name} ({duration:.1f}s) log={log_path}", flush=True)
    return LaneResult(lane.name, returncode, duration, log_path)


def run_lanes(lanes: Iterable[Lane], log_dir: Path, workers: int) -> list[LaneResult]:
    selected = list(lanes)
    if not selected:
        return []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, workers)) as executor:
        futures = [executor.submit(run_lane, lane, log_dir) for lane in selected]
        results = [future.result() for future in concurrent.futures.as_completed(futures)]
    failures = [result for result in results if not result.passed]
    if failures:
        for failure in failures:
            print(f"\n--- {failure.name} tail ---", file=sys.stderr)
            lines = failure.log_path.read_text(encoding="utf-8", errors="replace").splitlines()
            print("\n".join(lines[-120:]), file=sys.stderr)
        raise QualificationError(
            "qualification lanes failed: " + ", ".join(failure.name for failure in failures)
        )
    return sorted(results, key=lambda result: result.name)


def bash(script: str) -> tuple[str, ...]:
    return ("bash", "-euo", "pipefail", "-c", script)


def ordinary_lanes(run_root: Path) -> list[Lane]:
    empty_artifacts = run_root / "no-artifacts"
    empty_artifacts.mkdir(parents=True, exist_ok=True)
    workload = run_root / "aerobag-cloud-workload-ci.json"
    workload_health = run_root / "aerobag-cloud-workload-ci-pipeline-health.json"
    python_tests = shlex.join(
        sorted(
            subprocess.check_output(
                [
                    "find",
                    "tools",
                    "product/preprocessor/scripts",
                    "-type",
                    "f",
                    "-name",
                    "test_*.py",
                    "-print",
                ],
                cwd=ROOT,
                text=True,
            ).split()
        )
    )
    return [
        Lane("ci-actionlint", ("go", "run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.7")),
        Lane("ci-reuse", (str(ROOT / "scripts/check-licenses.sh"),)),
        Lane("ci-rust-format", (str(ROOT / "scripts/check-rust-format.sh"),)),
        Lane(
            "ci-rust-shared",
            bash("cargo nextest run --workspace --profile ci --locked && cargo test --workspace --doc --locked"),
            ROOT / "crates",
        ),
        Lane(
            "ci-rust-core",
            bash("cargo nextest run --workspace --profile ci --locked && cargo test --workspace --doc --locked"),
            ROOT / "ui/core-rust",
            {"AEROBAG_ARTIFACT_READ_PATH": str(empty_artifacts)},
        ),
        Lane(
            "ci-rust-services",
            bash(
                "cargo test --workspace --all-features --locked"
                f" && cargo run --locked -p aerobag-cloud-server --features workload --bin aerobag-cloud-workload -- --profile ci --output {workload}"
                f" && python3 {ROOT / 'tools/verify_acs_workload_report.py'} {workload} --output {workload_health}"
            ),
            ROOT / "services",
        ),
        Lane(
            "ci-rust-preprocessor",
            bash("cargo nextest run --workspace --profile ci --locked && cargo test --workspace --doc --locked"),
            ROOT / "product/preprocessor",
        ),
        Lane(
            "ci-python",
            bash(
                f"mkdir -p {run_root / 'python-results'} && /usr/bin/python3 -m pytest {python_tests} "
                f"--junitxml={run_root / 'python-results/junit.xml'}"
            ),
        ),
    ]


def sequential_ci_lanes(run_root: Path) -> list[Lane]:
    ui_target = run_root / "ci-ui-target"
    if not CI_SIGNING_KEY.exists():
        CI_SIGNING_KEY.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "keytool", "-genkeypair", "-keystore", str(CI_SIGNING_KEY),
                "-storepass", "android", "-alias", "androiddebugkey",
                "-keypass", "android", "-keyalg", "RSA", "-keysize", "2048",
                "-validity", "10000", "-dname", "CN=Aerobag CI,OU=CI,O=Aerobag,C=US",
            ],
            cwd=ROOT,
            check=True,
            stdout=subprocess.DEVNULL,
        )
    return [
        Lane(
            "ci-web",
            bash(
                f"mkdir -p {shlex.quote(str(run_root / 'web-results'))}"
                " && npm --prefix ui/web-app run ci"
                " && node --test tools/e2e/*.test.mjs"
                " && git diff --exit-code -- ui/core-rust/schemas ui/web-app/src/generated"
            ),
            env={
                "AEROBAG_UI_TARGET_ROOT": str(ui_target),
                "AEROBAG_ARTIFACT_READ_PATH": str(run_root / "no-artifacts"),
                "AEROBAG_JUNIT_PATH": str(run_root / "web-results/junit.xml"),
            },
        ),
        Lane(
            "ci-android-jvm",
            (str(ROOT / "ui/android-app/scripts/test.sh"), "testDebugUnitTest"),
            env={
                **CI_SIGNING_ENVIRONMENT,
                "AEROBAG_UI_TARGET_ROOT": str(ui_target),
                "ANDROID_BUILD_NATIVE_LIBRARIES": "false",
                "AEROBAG_ARTIFACT_READ_PATH": str(run_root / "no-artifacts"),
            },
        ),
    ]


def prepare_inputs(run_root: Path) -> tuple[Path, Path, Path]:
    fixtures = run_root / "test-artifacts"
    subprocess.run(
        [
            "python3", "tools/ci/fetch_test_artifacts.py",
            "--fixture", "release-journey-publication",
            "--fixture", "android-smoke-publication",
            "--fixture", "android-rotation-live-feed",
            "--fixture", "nav-db-advance",
            "--destination", str(fixtures),
        ],
        cwd=ROOT,
        check=True,
    )
    for fixture_name in ("android-smoke-publication", "nav-db-advance"):
        subprocess.run(
            [
                "python3",
                "tools/ci/verify_nav_db_fixture_contracts.py",
                "--fixture-root",
                str(fixtures),
                "--fixture",
                fixture_name,
            ],
            cwd=ROOT,
            check=True,
        )
    source = fixtures / "e2e/release-journey-publication"
    materialized = run_root / "release-journey-materialized"
    subprocess.run(
        [
            "python3", "tools/ci/materialize_release_journey_fixture.py",
            "--source", str(source), "--output", str(materialized),
        ],
        cwd=ROOT,
        check=True,
    )
    apps = run_root / "release-e2e-apps"
    build_env = lane_environment(
        {
            **CI_SIGNING_ENVIRONMENT,
            "CI": "1",
            "AEROBAG_UI_TARGET_ROOT": str(run_root / "release-ui-target"),
            "ANDROID_TARGET_ABIS": "x86_64",
        }
    )
    subprocess.run(
        [str(ROOT / "ui/web-app/scripts/install-binaryen-wasm-opt.sh")],
        cwd=ROOT,
        env=build_env,
        check=True,
    )
    subprocess.run(
        [str(ROOT / "tools/ci/install_wasm_bindgen.sh")],
        cwd=ROOT,
        env=build_env,
        check=True,
    )
    subprocess.run(
        [str(ROOT / "tools/ci/build_release_e2e_apps.sh"), str(apps)],
        cwd=ROOT,
        env=build_env,
        check=True,
    )
    return fixtures, materialized / "fixture.json", apps


def web_lane(
    priority: str,
    run_root: Path,
    fixture: Path,
    apps: Path,
    repetitions: int,
) -> Lane:
    index = PRIORITIES.index(priority)
    package_port = 21_000 + index
    cloud_port = 21_100 + index
    state = run_root / f"lab-web-{priority}"
    env = {
        "AEROBAG_RELEASE_JOURNEY_FIXTURE": str(fixture),
        "AEROBAG_RELEASE_JOURNEY_APP_ARTIFACTS_DIR": str(apps),
        "AEROBAG_RELEASE_JOURNEY_WEB_DIST": str(apps / "web-dist"),
        "AEROBAG_RELEASE_JOURNEY_SERVE_WEB_DIST": "1",
        "AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY": "1",
        "AEROBAG_RELEASE_JOURNEY_REPETITIONS": str(repetitions),
        "AEROBAG_E2E_URL": f"http://127.0.0.1:{package_port}/",
        "AEROBAG_E2E_PEER_URL": f"http://127.0.0.1:{package_port}/",
        "AEROBAG_E2E_ARTIFACT_DIR": str(run_root / f"results-web-{priority}"),
        "AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR": str(state),
        "PACKAGE_SOURCE_PORT": str(package_port),
        "AEROBAG_E2E_CLOUD_PORT": str(cloud_port),
        "AEROBAG_CLOUD_SERVER_BIN": str(apps / "aerobag-cloud-serverd"),
    }
    return Lane(
        f"e2e-web-{priority}",
        bash(
            "status=0; (tools/e2e/release_journey_lab.sh fixture-start-web fresh"
            f" && tools/e2e/release_journey_lab.sh web-suite {priority}) || status=$?;"
            " tools/e2e/release_journey_lab.sh fixture-stop || true;"
            " tools/e2e/release_journey_lab.sh cloud-stop || true; exit \"$status\""
        ),
        env=env,
        timeout_seconds=10_800,
    )


def android_shard_lane(
    shard: int,
    run_root: Path,
    fixture: Path,
    apps: Path,
    repetitions: int,
) -> Lane:
    package_port = 21_200 + shard
    cloud_port = 21_300 + shard
    vnc_port = 5_940 + shard
    target = run_root / f"android-target-s{shard}"
    baseline_archive = run_root / "android-release-journey-baseline.tar"
    env = {
        "CI": "1",
        "KEEP_EMULATOR": "1",
        "EMULATOR_HEADLESS": "1",
        "VNC_PORT": str(vnc_port),
        "AVD_NAME": "aerobag34-local-candidate",
        "AVD_INSTANCE_NAME": (
            f"aerobag-local-{git('rev-parse', '--short=8', 'HEAD')}-s{shard}"
        ),
        "EMULATOR_READ_ONLY": "0",
        "AVD_PACKAGE_PATH": "system-images;android-34;aosp_atd;x86_64",
        "AEROBAG_UI_TARGET_ROOT": str(target),
        "AEROBAG_RELEASE_JOURNEY_FIXTURE": str(fixture),
        "AEROBAG_RELEASE_JOURNEY_APP_ARTIFACTS_DIR": str(apps),
        "AEROBAG_RELEASE_JOURNEY_WEB_DIST": str(apps / "web-dist"),
        "AEROBAG_RELEASE_JOURNEY_SERVE_WEB_DIST": "1",
        "AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY": "1",
        "AEROBAG_RELEASE_JOURNEY_REPETITIONS": str(repetitions),
        "AEROBAG_E2E_URL": f"http://127.0.0.1:{package_port}/",
        "AEROBAG_E2E_PEER_URL": f"http://127.0.0.1:{package_port}/",
        "AEROBAG_E2E_ARTIFACT_DIR": str(
            run_root / f"results-android-s{shard}"
        ),
        "AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR": str(
            run_root / f"lab-android-s{shard}"
        ),
        "PACKAGE_SOURCE_PORT": str(package_port),
        "AEROBAG_E2E_CLOUD_PORT": str(cloud_port),
        "AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT": "18093",
        "ANDROID_PACKAGE_SOURCE_DEVICE_PORT": "18093",
        "AEROBAG_ANDROID_CLOUD_DEVICE_PORT": "18094",
        "AEROBAG_CLOUD_SERVER_BIN": str(apps / "aerobag-cloud-serverd"),
        "AEROBAG_ANDROID_BASELINE_ARCHIVE": str(baseline_archive),
    }
    suite = (
        "tools/e2e/release_journey_lab.sh android-suite-shard "
        f"all {shard} {ANDROID_SHARDS}"
    )
    setup = (
        "tools/e2e/release_journey_lab.sh fixture-start-web empty"
        " && tools/e2e/release_journey_lab.sh android-boot-install"
        f" {apps / 'aerobag-release-e2e.apk'}"
        f" {apps / 'aerobag-e2e-driver.apk'}"
        f" && {suite}"
    )
    cleanup = (
        ") || status=$?; ui/android-app/scripts/stop_emulator_stack.sh || true;"
        " tools/e2e/release_journey_lab.sh fixture-stop || true;"
        " tools/e2e/release_journey_lab.sh cloud-stop || true;"
        " avdmanager delete avd --name \"$AVD_INSTANCE_NAME\" >/dev/null 2>&1 || true;"
        " exit \"$status\""
    )
    return Lane(
        f"e2e-android-s{shard}",
        bash("status=0; (" + setup + cleanup),
        env=env,
        timeout_seconds=21_600,
    )


def android_baseline_lane(
    run_root: Path,
    fixture: Path,
    apps: Path,
) -> Lane:
    package_port = 21_190
    cloud_port = 21_191
    baseline_archive = run_root / "android-release-journey-baseline.tar"
    avd = f"aerobag-local-{git('rev-parse', '--short=8', 'HEAD')}-baseline"
    env = {
        "CI": "1",
        "KEEP_EMULATOR": "1",
        "EMULATOR_HEADLESS": "1",
        "VNC_PORT": "5939",
        "AVD_NAME": "aerobag34-local-candidate",
        "AVD_INSTANCE_NAME": avd,
        "EMULATOR_READ_ONLY": "0",
        "AVD_PACKAGE_PATH": "system-images;android-34;aosp_atd;x86_64",
        "AEROBAG_UI_TARGET_ROOT": str(run_root / "android-target-baseline"),
        "AEROBAG_RELEASE_JOURNEY_FIXTURE": str(fixture),
        "AEROBAG_RELEASE_JOURNEY_APP_ARTIFACTS_DIR": str(apps),
        "AEROBAG_RELEASE_JOURNEY_WEB_DIST": str(apps / "web-dist"),
        "AEROBAG_RELEASE_JOURNEY_SERVE_WEB_DIST": "1",
        "AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY": "1",
        "AEROBAG_RELEASE_JOURNEY_REPETITIONS": "1",
        "AEROBAG_E2E_URL": f"http://127.0.0.1:{package_port}/",
        "AEROBAG_E2E_PEER_URL": f"http://127.0.0.1:{package_port}/",
        "AEROBAG_E2E_ARTIFACT_DIR": str(run_root / "results-android-baseline"),
        "AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR": str(run_root / "lab-android-baseline"),
        "PACKAGE_SOURCE_PORT": str(package_port),
        "AEROBAG_E2E_CLOUD_PORT": str(cloud_port),
        "AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT": "18093",
        "ANDROID_PACKAGE_SOURCE_DEVICE_PORT": "18093",
        "AEROBAG_ANDROID_CLOUD_DEVICE_PORT": "18094",
        "AEROBAG_CLOUD_SERVER_BIN": str(apps / "aerobag-cloud-serverd"),
    }
    setup = (
        "tools/e2e/release_journey_lab.sh fixture-start-web empty"
        " && ui/android-app/scripts/run_e2e_ci.sh --headless --keep-emulator"
        " --skip-system-image-install --no-package-server"
        f" --apk {apps / 'aerobag-release-e2e.apk'}"
        f" --driver-apk {apps / 'aerobag-e2e-driver.apk'}"
        f" --release-fixture {fixture} --test shared.startup-navigation"
        f" && tools/e2e/release_journey_lab.sh android-baseline-save {baseline_archive}"
    )
    cleanup = (
        ") || status=$?; ui/android-app/scripts/stop_emulator_stack.sh || true;"
        " tools/e2e/release_journey_lab.sh fixture-stop || true;"
        f" avdmanager delete avd --name {shlex.quote(avd)} >/dev/null 2>&1 || true;"
        " exit \"$status\""
    )
    return Lane(
        "e2e-android-baseline",
        bash("status=0; (" + setup + cleanup),
        env=env,
        timeout_seconds=3_600,
    )


def native_lane(test_id: str, index: int, run_root: Path, fixtures: Path, apps: Path) -> Lane:
    vnc_port = 5_950 + index
    package_port = 21_400 + index
    target = run_root / f"native-target-{index}"
    avd = f"aerobag-local-native-{git('rev-parse', '--short=8', 'HEAD')}-{index}"
    env = {
        "CI": "1",
        "KEEP_EMULATOR": "0",
        "EMULATOR_HEADLESS": "1",
        "VNC_PORT": str(vnc_port),
        "AVD_NAME": "aerobag34-local-native",
        "AVD_INSTANCE_NAME": avd,
        "EMULATOR_READ_ONLY": "0",
        "AVD_PACKAGE_PATH": (
            "system-images;android-34;google_apis;x86_64"
            if test_id == "android.plate-first-render-smoke"
            else "system-images;android-34;aosp_atd;x86_64"
        ),
        "PACKAGE_SOURCE_PORT": str(package_port),
        "AEROBAG_UI_TARGET_ROOT": str(target),
        "AEROBAG_TEST_ARTIFACTS_ROOT": str(fixtures),
        "AEROBAG_ARTIFACT_READ_PATH": str(fixtures / "e2e/android-smoke-publication/published"),
        "AEROBAG_E2E_ARTIFACT_DIR": str(run_root / f"results-{test_id}"),
        "AEROBAG_E2E_APK": str(apps / "aerobag-release-e2e.apk"),
        "AEROBAG_E2E_DRIVER_APK": str(apps / "aerobag-e2e-driver.apk"),
    }
    command = shlex.join(
        [
            str(ROOT / "ui/android-app/scripts/run_e2e_ci.sh"),
            "--headless",
            "--skip-system-image-install",
            "--test",
            test_id,
        ]
    )
    return Lane(
        f"e2e-{test_id}",
        bash(
            f"status=0; {command} || status=$?;"
            f" avdmanager delete avd --name {shlex.quote(avd)} >/dev/null 2>&1 || true;"
            " exit \"$status\""
        ),
        env=env,
        timeout_seconds=3_600,
    )


def auxiliary_lanes(run_root: Path, fixtures: Path) -> list[Lane]:
    nav_target = run_root / "nav-rollover-target"
    chrome_avd = f"aerobag-local-chrome-{git('rev-parse', '--short=8', 'HEAD')}"
    return [
        Lane(
            "e2e-web-nav-db-rollover",
            (
                "npm", "--prefix", "ui/web-app", "run", "e2e:nav-db-rollover", "--",
                "--no-record", "--artifact-root", str(run_root / "results-nav-rollover"),
                "--run-id", "local-candidate",
            ),
            env={
                "AEROBAG_TEST_ARTIFACTS_ROOT": str(fixtures),
                "AEROBAG_UI_TARGET_ROOT": str(nav_target),
            },
            timeout_seconds=3_600,
        ),
        Lane(
            "e2e-android-chrome-live-feed",
            bash(
                "status=0;"
                f" {shlex.join([str(ROOT / 'ui/web-app/scripts/run_android_chrome_livefeed_e2e.sh'), '--headless', '--json'])}"
                " || status=$?;"
                f" avdmanager delete avd --name {shlex.quote(chrome_avd)} >/dev/null 2>&1 || true;"
                " exit \"$status\""
            ),
            env={
                "CI": "1",
                "KEEP_EMULATOR": "0",
                "EMULATOR_HEADLESS": "1",
                "VNC_PORT": "5960",
                "AVD_NAME": "aerobag34-local-chrome",
                "AVD_INSTANCE_NAME": chrome_avd,
                "EMULATOR_READ_ONLY": "0",
                "AEROBAG_TEST_ARTIFACTS_ROOT": str(fixtures),
                "AEROBAG_UI_TARGET_ROOT": str(run_root / "chrome-target"),
                "AEROBAG_ARTIFACT_READ_PATH": str(fixtures / "e2e/android-smoke-publication/published"),
                "AEROBAG_E2E_ARTIFACT_DIR": str(run_root / "results-android-chrome"),
            },
            timeout_seconds=3_600,
        ),
    ]


def write_receipt(
    commit: str,
    started_at: str,
    run_root: Path,
    apps: Path,
    results: list[LaneResult],
    repetitions: int,
) -> Path:
    manifest = json.loads((apps / "build-manifest.json").read_text(encoding="utf-8"))
    receipt = {
        "schema_version": 1,
        "commit": commit,
        "status": "passed",
        "started_at": started_at,
        "completed_at": datetime.now(timezone.utc).isoformat(),
        "repetitions": repetitions,
        "workflow_identity": workflow_identity(),
        "build_manifest": manifest,
        "run_root": str(run_root),
        "lanes": [
            {
                "name": result.name,
                "duration_seconds": round(result.duration_seconds, 3),
                "log": str(result.log_path),
            }
            for result in sorted(results, key=lambda result: result.name)
        ],
    }
    path = receipt_path(commit)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)
    return path


def initial_journey_lane_groups(
    run_root: Path,
    fixtures: Path,
    fixture: Path,
    apps: Path,
    repetitions: int,
) -> tuple[tuple[str, list[Lane]], ...]:
    return (
        (
            "Running web priorities in parallel",
            [
                web_lane(priority, run_root, fixture, apps, repetitions)
                for priority in PRIORITIES
            ],
        ),
        (
            "Running NAV rollover in an isolated local phase",
            [auxiliary_lanes(run_root, fixtures)[0]],
        ),
        (
            "Preparing the Android baseline in an isolated local phase",
            [android_baseline_lane(run_root, fixture, apps)],
        ),
    )


def main() -> int:
    args = parse_args()
    if args.repetitions < 1:
        raise QualificationError("--repetitions must be positive")
    if args.android_workers < 1:
        raise QualificationError("--android-workers must be positive")
    commit = assert_clean_commit()
    receipt = valid_receipt(commit)
    if args.check:
        if receipt is None:
            raise QualificationError(f"no valid local qualification receipt for {commit}")
        print(f"Local candidate qualification passed: {receipt_path(commit)}")
        return 0
    if receipt is not None and receipt.get("repetitions") == args.repetitions:
        print(f"Local candidate qualification already passed: {receipt_path(commit)}")
        return 0

    started_at = datetime.now(timezone.utc).isoformat()
    run_root = Path(tempfile.gettempdir()) / f"aerobag-local-candidate-{commit[:12]}"
    if run_root.exists():
        shutil.rmtree(run_root)
    run_root.mkdir(parents=True)
    logs = run_root / "logs"
    results: list[LaneResult] = []

    prepare_gradle_wrapper_cache(run_root)
    prepare_environment()

    print("Running ordinary CI lanes in parallel", flush=True)
    results.extend(run_lanes(ordinary_lanes(run_root), logs, min(args.jobs, 7)))
    for lane in sequential_ci_lanes(run_root):
        results.extend(run_lanes([lane], logs, 1))

    if git("status", "--porcelain"):
        raise QualificationError("CI generated tracked source changes")

    print("Building one immutable app bundle and fixture", flush=True)
    fixtures, fixture, apps = prepare_inputs(run_root)

    # GitHub gives these GUI-heavy jobs independent runners. Running them beside
    # the web browsers on one host can pause every UI process at once and create
    # failures that cannot occur between isolated hosted runners.
    for label, lanes in initial_journey_lane_groups(
        run_root, fixtures, fixture, apps, args.repetitions
    ):
        print(label, flush=True)
        results.extend(run_lanes(lanes, logs, min(args.jobs, len(lanes))))

    android_lane_count = ANDROID_SHARDS
    android_workers = min(args.android_workers, android_lane_count)
    print(
        f"Running {android_lane_count} fresh Android shards with "
        f"{android_workers} local emulator worker(s)",
        flush=True,
    )
    android_lanes = [
        android_shard_lane(
            shard, run_root, fixture, apps, args.repetitions
        )
        for shard in range(ANDROID_SHARDS)
    ]
    results.extend(run_lanes(android_lanes, logs, android_workers))

    print(
        f"Running native Android and Chrome lanes with {android_workers} local emulator worker(s)",
        flush=True,
    )
    native = [
        native_lane(test_id, index, run_root, fixtures, apps)
        for index, test_id in enumerate(NATIVE_TESTS)
    ]
    native.append(auxiliary_lanes(run_root, fixtures)[1])
    results.extend(run_lanes(native, logs, min(android_workers, len(native))))

    path = write_receipt(commit, started_at, run_root, apps, results, args.repetitions)
    print(f"Local candidate qualification passed: {path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (QualificationError, subprocess.CalledProcessError) as error:
        print(f"local candidate qualification: {error}", file=sys.stderr)
        raise SystemExit(2)
