# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
import os
from pathlib import Path
import subprocess
import sys
from unittest import mock

import pytest

import emulator_disk as disk


HOSTED = {
    "GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux",
    "ANDROID_HOME": "/usr/local/lib/android/sdk",
}


def test_cleanup_is_only_allowed_on_known_disposable_hosted_sdk():
    assert disk.hosted_sdk(HOSTED) == Path(HOSTED["ANDROID_HOME"])
    for changed in [
        {"GITHUB_ACTIONS": "false"}, {"RUNNER_ENVIRONMENT": "self-hosted"},
        {"RUNNER_OS": "macOS"}, {"ANDROID_HOME": "/"},
        {"ANDROID_HOME": "/root/android-sdk"}, {"ANDROID_HOME": ""},
        {"ANDROID_SDK_ROOT": "/other/sdk"},
    ]:
        with pytest.raises(ValueError):
            disk.hosted_sdk({**HOSTED, **changed})
    with pytest.raises(ValueError):
        disk.hosted_sdk({})


def test_cleanup_removes_only_build_packages_and_is_idempotent(tmp_path):
    runtime = ["emulator", "platform-tools", "cmdline-tools", "system-images", "licenses", "unknown"]
    build = ["ndk", "platforms", "build-tools", "cmake"]
    for directory in runtime + build:
        (tmp_path / directory).mkdir()
        (tmp_path / directory / "keep-or-remove").write_text(directory)
    assert set(disk.clean_build_tools(tmp_path)) == set(build)
    for directory in runtime:
        assert (tmp_path / directory / "keep-or-remove").read_text() == directory
    assert disk.clean_build_tools(tmp_path) == []


def test_cleanup_rejects_symlinks_before_removing_anything(tmp_path):
    (tmp_path / "ndk").mkdir()
    (tmp_path / "keep").mkdir()
    (tmp_path / "keep/data").write_text("not ours")
    (tmp_path / "platforms").symlink_to(tmp_path / "keep", target_is_directory=True)
    with pytest.raises(ValueError, match="symlink"):
        disk.clean_build_tools(tmp_path)
    assert (tmp_path / "ndk").exists()
    assert (tmp_path / "keep/data").read_text() == "not ours"


def test_report_records_disk_headroom_and_allocated_bytes_without_cleanup(tmp_path, capsys):
    (tmp_path / "keep").write_text("data")
    summary = tmp_path / "summary.md"
    with mock.patch.object(disk, "clean_build_tools") as cleanup:
        report = disk.report("after-test", {"avd": tmp_path}, summary, measure_sizes=True)
    cleanup.assert_not_called()
    assert report["phase"] == "after-test"
    row = report["paths"]["avd"]
    assert row["allocated_bytes"] > 0
    assert row["free_bytes"] > 0
    assert row["total_bytes"] >= row["free_bytes"]
    assert json.loads(capsys.readouterr().out) == report
    assert "after-test" in summary.read_text()
    assert "Free GiB" in summary.read_text()
    assert (tmp_path / "keep").read_text() == "data"


def test_default_report_never_walks_directories(tmp_path):
    with mock.patch.object(disk.subprocess, "run", side_effect=AssertionError("recursive scan")):
        report = disk.report("before-boot", {"avd": tmp_path})
    row = report["paths"]["avd"]
    assert row["free_bytes"] > 0
    assert row["allocated_bytes"] is None


def test_timed_out_scan_keeps_free_space_and_skips_remaining_scans(tmp_path, capsys):
    summary = tmp_path / "summary.md"
    with mock.patch.object(disk.subprocess, "run", side_effect=subprocess.TimeoutExpired("du", 5)) as scan:
        report = disk.report("after-test", {"sdk": tmp_path, "avd": tmp_path}, summary, measure_sizes=True)
    scan.assert_called_once()
    assert 0 < scan.call_args.kwargs["timeout"] <= 5
    for row in report["paths"].values():
        assert row["free_bytes"] > 0
        assert row["allocated_bytes"] is None
        assert row["size_error"]
    assert "timed out" in report["paths"]["sdk"]["size_error"]
    assert "budget" in report["paths"]["avd"]["size_error"]
    output = capsys.readouterr()
    assert json.loads(output.out) == report
    assert "timed out" in output.err
    assert "unavailable" in summary.read_text()


@pytest.mark.parametrize("outcome", [
    FileNotFoundError("du missing"),
    subprocess.CalledProcessError(1, "du", stderr="cannot read directory"),
    subprocess.CompletedProcess("du", 0, stdout=""),
    subprocess.CompletedProcess("du", 0, stdout="not-a-number path"),
])
def test_scan_errors_are_explicit_unknowns_not_zero_bytes(tmp_path, outcome):
    with mock.patch.object(disk.subprocess, "run") as scan:
        if isinstance(outcome, Exception):
            scan.side_effect = outcome
        else:
            scan.return_value = outcome
        report = disk.report("after-test", {"sdk": tmp_path}, measure_sizes=True)
    row = report["paths"]["sdk"]
    assert row["free_bytes"] > 0
    assert row["allocated_bytes"] is None
    assert row["size_error"]


def test_size_scans_share_one_budget(tmp_path):
    with mock.patch.object(disk.time, "monotonic", side_effect=[10, 10, 14, 16]), mock.patch.object(
        disk.subprocess, "run", return_value=subprocess.CompletedProcess("du", 0, stdout="4096 path"),
    ) as scan:
        report = disk.report("after-test", {"workspace": tmp_path, "sdk": tmp_path, "avd": tmp_path}, measure_sizes=True)
    assert [call.kwargs["timeout"] for call in scan.call_args_list] == [5, 1]
    assert report["paths"]["sdk"]["allocated_bytes"] == 4096
    assert report["paths"]["avd"]["allocated_bytes"] is None
    assert "budget" in report["paths"]["avd"]["size_error"]


@pytest.mark.parametrize("headroom_unavailable", [False, True])
def test_measurement_and_summary_failures_do_not_prevent_real_cleanup(tmp_path, monkeypatch, capsys, headroom_unavailable):
    sdk = tmp_path / "sdk"
    (sdk / "ndk").mkdir(parents=True)
    (sdk / "emulator").mkdir()
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(disk, "HOSTED_SDK", sdk)
    monkeypatch.setattr(sys, "argv", ["emulator_disk.py", "--phase", "before-sdk-install", "--clean-unused-sdk"])
    # A directory cannot be opened as a summary file, even when tests run as root.
    with mock.patch.dict(os.environ, {**HOSTED, "ANDROID_HOME": str(sdk), "GITHUB_STEP_SUMMARY": str(tmp_path)}, clear=True), mock.patch.object(
        disk.subprocess, "run", side_effect=subprocess.TimeoutExpired("du", 60),
    ) as scan, mock.patch.object(
        disk.shutil, "disk_usage",
        side_effect=OSError("statvfs unavailable") if headroom_unavailable else disk.shutil.disk_usage,
    ):
        disk.main()
    scan.assert_not_called()
    assert not (sdk / "ndk").exists()
    assert (sdk / "emulator").exists()
    assert "summary" in capsys.readouterr().err


def test_cleanup_errors_still_propagate(tmp_path, monkeypatch):
    monkeypatch.setattr(sys, "argv", ["emulator_disk.py", "--phase", "before-sdk-install", "--clean-unused-sdk"])
    with mock.patch.object(disk, "hosted_sdk", return_value=tmp_path), mock.patch.object(disk, "report"), mock.patch.object(
        disk, "clean_build_tools", side_effect=PermissionError("cannot remove NDK"),
    ):
        with pytest.raises(PermissionError, match="cannot remove NDK"):
            disk.main()


def test_cli_succeeds_without_du_and_reports_missing_diagnostic(tmp_path):
    result = subprocess.run(
        [sys.executable, str(Path(disk.__file__).resolve()), "--phase", "after-test", "--measure-sizes"],
        cwd=tmp_path, env={"PATH": "", "HOME": str(tmp_path), "ANDROID_HOME": str(tmp_path)},
        text=True, capture_output=True, timeout=10,
    )
    assert result.returncode == 0, result.stderr
    report = json.loads(result.stdout)
    assert report["paths"]["android_sdk"]["free_bytes"] > 0
    assert report["paths"]["android_sdk"]["allocated_bytes"] is None
    assert "du" in report["paths"]["android_sdk"]["size_error"]
