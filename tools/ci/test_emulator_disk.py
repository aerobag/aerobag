# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import json
from pathlib import Path
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
        report = disk.report("before-boot", {"avd": tmp_path}, summary)
    cleanup.assert_not_called()
    assert report["phase"] == "before-boot"
    row = report["paths"]["avd"]
    assert row["allocated_bytes"] > 0
    assert row["free_bytes"] > 0
    assert row["total_bytes"] >= row["free_bytes"]
    assert json.loads(capsys.readouterr().out) == report
    assert "before-boot" in summary.read_text()
    assert "Free GiB" in summary.read_text()
    assert (tmp_path / "keep").read_text() == "data"
