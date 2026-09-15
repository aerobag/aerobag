# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import subprocess
from unittest import mock

import pytest

import install_android_sdk as installer


PACKAGES = ["emulator", "system-images;android-34;aosp_atd;x86_64"]


def result(code, output):
    return subprocess.CompletedProcess(["sdkmanager", *PACKAGES], code, stdout=output)


def test_corrupt_sdk_download_recovers_without_replaying_any_test(capsys):
    with mock.patch.object(installer.subprocess, "run", side_effect=[
        result(1, installer.CORRUPT_DOWNLOAD + ".\n"), result(0, "installed\n"),
    ]) as run, mock.patch.object(installer.time, "sleep") as sleep:
        assert installer.install(PACKAGES) == 0
    assert run.call_count == 2
    for call in run.call_args_list:
        assert call.args == (["sdkmanager", *PACKAGES],)
        assert 0 < call.kwargs["timeout"] <= installer.INSTALL_DEADLINE_SECONDS
    sleep.assert_called_once_with(2)
    output = capsys.readouterr().out
    assert installer.CORRUPT_DOWNLOAD in output
    assert "attempt 2/3" in output
    assert "installed" in output


@pytest.mark.parametrize("code,output", [
    (0, "installed\n"), (1, "Failed to find package 'misspelled'\n"),
    (1, "No space left on device\n"), (2, "unknown failure\n"),
])
def test_success_and_unclassified_errors_are_not_retried(code, output):
    with mock.patch.object(installer.subprocess, "run", return_value=result(code, output)) as run, \
            mock.patch.object(installer.time, "sleep") as sleep:
        assert installer.install(PACKAGES) == code
    assert run.call_count == 1
    sleep.assert_not_called()


def test_persistent_corruption_fails_after_three_attempts():
    with mock.patch.object(installer.subprocess, "run", return_value=result(1, installer.CORRUPT_DOWNLOAD)) as run, \
            mock.patch.object(installer.time, "sleep") as sleep:
        assert installer.install(PACKAGES) == 1
    assert run.call_count == 3
    assert sleep.call_args_list == [mock.call(2), mock.call(4)]


def test_deadline_is_shared_across_attempts():
    with mock.patch.object(installer.subprocess, "run", return_value=result(1, installer.CORRUPT_DOWNLOAD)) as run, \
            mock.patch.object(installer.time, "monotonic", side_effect=[0, 0, 599]), \
            mock.patch.object(installer.time, "sleep") as sleep:
        assert installer.install(PACKAGES) == 1
    assert run.call_count == 1
    sleep.assert_not_called()


def test_timeout_retains_output_and_does_not_retry(capsys):
    with mock.patch.object(installer.subprocess, "run", side_effect=subprocess.TimeoutExpired(
        "sdkmanager", 600, output=b"download still pending\n",
    )) as run:
        assert installer.install(PACKAGES) == 124
    assert run.call_count == 1
    assert "download still pending" in capsys.readouterr().out


@pytest.mark.parametrize("packages", [[], ["--update"], ["--uninstall", "emulator"]])
def test_wrapper_accepts_only_explicit_install_packages(packages):
    with pytest.raises(ValueError):
        installer.install(packages)
