#!/usr/bin/env python3

import os
import subprocess
import sys


PACKAGES = [
    "ca-certificates",
    "exiftool",
    "gdal-bin",
    "ghostscript",
    "imagemagick",
    "jq",
    "perl",
    "python3",
    "python3-bs4",
    "python3-elementpath",
    "python3-gdal",
    "python3-glob2",
    "python3-numpy",
    "python3-pypdf",
    "python3-regex",
    "python3-tqdm",
    "python3-urllib3",
    "sqlite3",
    "unzip",
    "zip",
]


def run(cmd, env):
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, check=True, env=env)


def main():
    if os.geteuid() != 0:
        print("run this as root, for example:", file=sys.stderr)
        print("  sudo python3 legacy-capture/provision_legacy_host.py", file=sys.stderr)
        raise SystemExit(1)

    env = os.environ.copy()
    env.setdefault("DEBIAN_FRONTEND", "noninteractive")

    print("Installing legacy host dependencies", flush=True)
    run(["apt-get", "update"], env)
    run(["apt-get", "install", "-y", *PACKAGES], env)
    print("legacy host dependencies installed", flush=True)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as exc:
        raise SystemExit(exc.returncode) from exc
