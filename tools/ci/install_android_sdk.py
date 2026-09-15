#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Install SDK dependencies, retrying only a diagnosed corrupt-download failure."""

from __future__ import annotations

import subprocess
import sys
import time

MAX_ATTEMPTS = 3
INSTALL_DEADLINE_SECONDS = 600
CORRUPT_DOWNLOAD = "Error reading Zip content from a SeekableByteChannel"


def install(packages: list[str]) -> int:
    if not packages or any(package.startswith("-") for package in packages):
        raise ValueError("expected SDK package identifiers, not sdkmanager options")
    command = ["sdkmanager", *packages]
    deadline = time.monotonic() + INSTALL_DEADLINE_SECONDS
    for attempt in range(1, MAX_ATTEMPTS + 1):
        print(f"Android SDK install attempt {attempt}/{MAX_ATTEMPTS}: {packages}", flush=True)
        try:
            result = subprocess.run(
                command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                timeout=max(0, deadline - time.monotonic()), check=False,
            )
        except subprocess.TimeoutExpired as error:
            output = error.stdout or b""
            print(output.decode(errors="replace") if isinstance(output, bytes) else output, flush=True)
            print("Android SDK install exceeded its total deadline; not retrying", file=sys.stderr)
            return 124
        except OSError as error:
            print(f"Android SDK installer unavailable: {error}", file=sys.stderr)
            return 127
        print(result.stdout, end="", flush=True)
        if result.returncode == 0:
            return 0
        if CORRUPT_DOWNLOAD not in result.stdout or attempt == MAX_ATTEMPTS:
            return result.returncode
        delay = 2 ** attempt
        if time.monotonic() + delay >= deadline:
            return result.returncode
        # sdkmanager owns package-install transactions and their scratch files.
        # Do not delete or uninstall an existing SDK to recover a download.
        print(f"Corrupt SDK download; retrying dependency installation in {delay}s", flush=True)
        time.sleep(delay)
    raise AssertionError("unreachable")


if __name__ == "__main__":
    try:
        raise SystemExit(install(sys.argv[1:]))
    except ValueError as error:
        print(error, file=sys.stderr)
        raise SystemExit(2)
