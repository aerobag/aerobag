#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Check generated UI sources without overwriting the working tree."""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WIRE_OUTPUTS = {
    "--web-out": "nexradOverlayWire.ts",
    "--cloud-web-out": "cloudWire.ts",
    "--home-page-web-out": "homePageWire.ts",
    "--nav-query-web-out": "navQueryWire.ts",
    "--session-page-web-out": "sessionPageWire.ts",
    "--session-update-web-out": "sessionUpdateWire.ts",
    "--session-work-web-out": "sessionWorkWire.ts",
    "--session-update-conformance-web-out": "sessionUpdateConformance.json",
    "--ui-geometry-conformance-web-out": "uiGeometryConformance.json",
}


def compare(generated: Path, checked_in: Path) -> list[str]:
    return [
        str(checked_in / file.name)
        for file in sorted(generated.iterdir())
        if file.is_file() and (
            not (checked_in / file.name).is_file()
            or file.read_bytes() != (checked_in / file.name).read_bytes()
        )
    ]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="aerobag-generated-ui-") as directory:
        output = Path(directory)
        subprocess.run([
            "cargo", "+1.94.1", "run", "--quiet", "--locked",
            "--manifest-path", str(ROOT / "ui/core-rust/Cargo.toml"),
            "-p", "app-ui-contracts", "--features", "schema",
            "--bin", "generate-ui-contract-schemas", "--",
            "--out-dir", str(output / "schemas"),
        ], cwd=ROOT, check=True)
        stale = compare(output / "schemas", ROOT / "ui/core-rust/schemas")
        # The wire generator reads the checked-in schemas. A schema mismatch is
        # already a failure; comparing both stages also identifies stale wires.
        command = ["node", "tools/generate-ui-wire-types.mjs"]
        for flag, name in WIRE_OUTPUTS.items():
            command.extend([flag, str(output / "web" / name)])
        command.extend(["--android-out", str(output / "android-wire")])
        subprocess.run(command, cwd=ROOT, check=True)
        subprocess.run([
            "node", "tools/generate-nav-symbols.mjs",
            "--web-out", str(output / "web/navSymbols.ts"),
            "--android-out", str(output / "android-symbols"),
        ], cwd=ROOT, check=True)
        stale.extend(compare(output / "web", ROOT / "ui/web-app/src/generated"))
        if stale:
            print("Generated UI sources are stale:\n" + "\n".join(stale))
            return 1
    print("Generated UI schemas, wire types, conformance data and symbols match")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
