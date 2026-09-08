# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import hashlib
import json
from pathlib import Path
from unittest import mock
import zipfile

import pytest

import install_test_browser as installer


@pytest.fixture
def browser_archive(tmp_path, monkeypatch):
    archive = tmp_path / "browser.zip"
    entry = zipfile.ZipInfo("chrome-linux64/chrome")
    entry.external_attr = 0o100755 << 16
    with zipfile.ZipFile(archive, "w") as zipped:
        zipped.writestr(entry, "test browser")
    lock = tmp_path / "lock.json"
    lock.write_text(json.dumps({
        "schema_version": 1, "platform": "linux64", "version": "1.2.3.4",
        "url": "https://invalid.example/browser.zip",
        "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
    }))
    monkeypatch.setattr(installer, "LOCK", lock)
    return archive, lock


def test_install_verifies_archive_permissions_version_and_reuses_cache(tmp_path, browser_archive):
    archive, _ = browser_archive
    with mock.patch.object(installer.subprocess, "check_output", return_value="Chrome 1.2.3.4\n") as version:
        binary = installer.install(tmp_path / "cache", archive)
        assert binary.read_text() == "test browser"
        assert binary.stat().st_mode & 0o777 == 0o755
        # A cached installation does not need to read/download an archive.
        assert installer.install(tmp_path / "cache", Path("missing.zip")) == binary
    assert version.call_count == 2
    version.assert_called_with([str(binary), "--version"], text=True, timeout=15)


def test_checksum_failure_never_publishes_cache(tmp_path, browser_archive):
    archive, _ = browser_archive
    archive.write_bytes(b"corrupted")
    with pytest.raises(ValueError, match="checksum mismatch"):
        installer.install(tmp_path / "cache", archive)
    assert list((tmp_path / "cache").iterdir()) == []


def test_unsafe_archive_never_escapes_cache(tmp_path, browser_archive):
    archive, lock = browser_archive
    with zipfile.ZipFile(archive, "w") as zipped:
        zipped.writestr("../../../escaped", "bad")
    settings = json.loads(lock.read_text())
    settings["sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
    lock.write_text(json.dumps(settings))
    with pytest.raises(ValueError, match="unsafe path"):
        installer.install(tmp_path / "cache", archive)
    assert not (tmp_path / "escaped").exists()
    assert list((tmp_path / "cache").iterdir()) == []


def test_unexpected_cache_is_preserved(tmp_path, browser_archive):
    archive, _ = browser_archive
    existing = tmp_path / "cache/1.2.3.4"
    existing.mkdir(parents=True)
    (existing / "keep").write_text("existing data")
    with pytest.raises(OSError):
        installer.install(tmp_path / "cache", archive)
    assert (existing / "keep").read_text() == "existing data"


def test_wrong_executable_version_is_rejected(tmp_path, browser_archive):
    archive, _ = browser_archive
    with mock.patch.object(installer.subprocess, "check_output", return_value="Chrome 9.9.9.9"):
        with pytest.raises(ValueError, match="version mismatch"):
            installer.install(tmp_path / "cache", archive)
