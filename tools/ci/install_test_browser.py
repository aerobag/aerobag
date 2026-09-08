#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Install the checksum-pinned Linux test browser into an explicit cache."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request
import zipfile

LOCK = Path(__file__).with_name("test-browser.lock.json")


def install(destination: Path, archive: Path | None = None) -> Path:
    lock = json.loads(LOCK.read_text())
    if lock["schema_version"] != 1 or lock["platform"] != "linux64":
        raise ValueError("unsupported test-browser lock")
    destination = destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    final = destination / lock["version"]
    executable = final / "chrome-linux64/chrome"
    marker = final / "archive.sha256"
    if not (executable.is_file() and marker.is_file() and marker.read_text() == lock["sha256"]):
        with tempfile.TemporaryDirectory(prefix="install-", dir=destination) as temporary:
            temporary = Path(temporary)
            source = archive or temporary / "chrome.zip"
            if archive is None:
                with urllib.request.urlopen(lock["url"], timeout=60) as response, source.open("wb") as output:
                    shutil.copyfileobj(response, output)
            with source.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            if digest != lock["sha256"]:
                raise ValueError("test-browser archive checksum mismatch")
            unpacked = temporary / "unpacked"
            with zipfile.ZipFile(source) as zipped:
                for entry in zipped.infolist():
                    path = unpacked / entry.filename
                    if not path.resolve().is_relative_to(unpacked.resolve()):
                        raise ValueError("test-browser archive contains an unsafe path")
                    zipped.extract(entry, unpacked)
                    if not entry.is_dir():
                        path.chmod((entry.external_attr >> 16) & 0o777 or 0o644)
            (unpacked / "archive.sha256").write_text(lock["sha256"])
            # Never overwrite a partial or unexpected cache; report it explicitly.
            unpacked.rename(final)
    version = subprocess.check_output([str(executable), "--version"], text=True, timeout=15).strip()
    if lock["version"] not in version.split():
        raise ValueError(f"test-browser version mismatch: {version}")
    return executable


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--archive", type=Path, help="use an already downloaded archive (still checksum-verified)")
    args = parser.parse_args()
    executable = install(args.destination, args.archive)
    print(executable)
    if os.environ.get("GITHUB_ENV"):
        with open(os.environ["GITHUB_ENV"], "a") as output:
            output.write(f"CHROME_BIN={executable}\n")


if __name__ == "__main__":
    main()
