#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Static audit for Android UI code that can invoke expensive work.

This is intentionally conservative. If a new expensive call site is legitimate,
classify it here instead of letting it appear as an unreviewed UI/input path.
"""

from __future__ import annotations

import re
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SRC_ROOT = ROOT / "app" / "src" / "main" / "java" / "org" / "aerobag" / "app"


@dataclass(frozen=True)
class AllowRule:
    path_suffix: str
    text: str | None
    classification: str
    reason: str
    context: tuple[str, ...] = ()

    def matches(self, rel_path: str, line: str, context_lines: tuple[str, ...]) -> bool:
        if not rel_path.endswith(self.path_suffix):
            return False
        if self.text is not None and self.text not in line:
            return False
        context_text = "\n".join(context_lines)
        return all(required in context_text for required in self.context)


@dataclass(frozen=True)
class Check:
    name: str
    pattern: re.Pattern[str]
    allow: tuple[AllowRule, ...]


CHECKS = (
    Check(
        name="ui_session_query",
        pattern=re.compile(r"\buiSession\.query[A-Za-z0-9_]*\("),
        allow=(
            AllowRule(
                "UiSessionWorkRunner.kt",
                None,
                "scheduled core work",
                "The runner is the only approved path for map overlay/selection session work.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.queryRasterTilePlanJson(",
                "fast model read",
                "Raster tile planning is a synchronous core model read with no resource paging.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.queryNexradOverlay(",
                "background IO/render",
                "NEXRAD is driven by a conflated render loop and executes on Dispatchers.IO.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.queryTerrainOverlay(",
                "background IO/render",
                "Terrain is driven by a conflated render loop and executes on Dispatchers.IO.",
            ),
        ),
    ),
    Check(
        name="native_get_in_session",
        pattern=re.compile(r"\b(get|bridge\.get)[A-Za-z0-9_]*InSession[A-Za-z0-9_]*\("),
        allow=(
            AllowRule(
                "domain/NativeBindings.kt",
                None,
                "native bridge declaration",
                "Bridge declarations do not execute work directly.",
            ),
            AllowRule(
                "domain/NativeAppCoreAdapter.kt",
                None,
                "session adapter",
                "NativeAppCoreAdapter is the platform boundary that wraps core calls.",
            ),
        ),
    ),
    Check(
        name="paged_session_operation",
        pattern=re.compile(r"\brunPagedSessionOperation(?:Element)?\b"),
        allow=(
            AllowRule(
                "domain/NavKvStore.kt",
                None,
                "resource-paging primitive",
                "NavKvStore owns the paging loop implementation.",
            ),
            AllowRule(
                "domain/NativeAppCoreAdapter.kt",
                None,
                "session adapter",
                "NativeAppCoreAdapter is the only approved caller of raw session paging.",
            ),
        ),
    ),
    Check(
        name="core_resource_fetch",
        pattern=re.compile(r"\bfetchCoreResource\("),
        allow=(
            AllowRule(
                "RuntimeFetch.kt",
                None,
                "platform IO adapter",
                "RuntimeFetch owns typed core resource fetching.",
            ),
            AllowRule(
                "UiSessionWorkRunner.kt",
                None,
                "scheduled core work",
                "Resource callbacks from this runner execute inside scheduled IO work.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "fetchCoreResource(",
                "background IO/render",
                "Map overlay resource fetches are only exposed through the named overlay worker helper.",
                ("private fun fetchMapOverlayCoreResource(",),
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "fetchCoreResource(",
                "background IO/render",
                "NEXRAD resource fetches are only exposed through the named NEXRAD worker helper.",
                ("private fun fetchNexradCoreResource(",),
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "fetchCoreResource(",
                "background IO/render",
                "Terrain resource fetches are only exposed through the named terrain worker helper.",
                ("private fun fetchTerrainCoreResource(",),
            ),
            AllowRule(
                "ChartsPage.kt",
                None,
                "background IO/render",
                "Plate asset and thumbnail loads use produceState with Dispatchers.IO.",
            ),
        ),
    ),
    Check(
        name="ui_session_resource_work",
        pattern=re.compile(
            r"\buiSession\.(?:chartAssetBytes|queryNexradOverlay|nexradTileBytes|queryTerrainOverlay|renderTerrainOverlayTile|queryMapOverlay|queryMapSelection|queryMapSelectionForNavRef)\("
        ),
        allow=(
            AllowRule(
                "UiSessionWorkRunner.kt",
                None,
                "scheduled core work",
                "Map overlay/selection resource-paging work must go through the session work runner.",
            ),
            AllowRule(
                "ChartsPage.kt",
                "uiSession.chartAssetBytes(",
                "background IO/render",
                "Chart asset bytes are loaded from produceState with Dispatchers.IO.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.queryNexradOverlay(",
                "background IO/render",
                "NEXRAD overlay queries run inside the conflated NEXRAD render loop.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.nexradTileBytes(",
                "background IO/render",
                "NEXRAD tile reads run inside the conflated NEXRAD render loop.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.queryTerrainOverlay(",
                "background IO/render",
                "Terrain overlay queries run inside the conflated terrain render loop.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                "uiSession.renderTerrainOverlayTile(",
                "background IO/render",
                "Terrain tile rendering runs inside the conflated terrain render loop.",
            ),
        ),
    ),
    Check(
        name="bitmap_decode",
        pattern=re.compile(r"\bBitmapFactory\.decode[A-Za-z0-9_]*\("),
        allow=(
            AllowRule(
                "TileLoading.kt",
                None,
                "background IO/render",
                "Raster tile decoding runs in the tile bitmap loader worker.",
            ),
            AllowRule(
                "ChartsPage.kt",
                None,
                "background IO/render",
                "Plate bitmap decoding runs in produceState with Dispatchers.IO.",
            ),
            AllowRule(
                "MapExplorerPage.kt",
                None,
                "background IO/render",
                "NEXRAD bitmap decoding runs inside its Dispatchers.IO render loop.",
            ),
        ),
    ),
    Check(
        name="zip_open",
        pattern=re.compile(r"\bZipFile\("),
        allow=(
            AllowRule(
                "domain/PackageZipStore.kt",
                None,
                "package IO adapter",
                "PackageZipStore is the only approved zip handle owner.",
            ),
        ),
    ),
    Check(
        name="http_connection",
        pattern=re.compile(r"\bURL\([^)]*\)\.openConnection\(\)|\.openConnection\(\) as HttpURLConnection"),
        allow=(
            AllowRule(
                "RuntimeFetch.kt",
                None,
                "platform IO adapter",
                "RuntimeFetch owns ad hoc HTTP resource fetches.",
            ),
            AllowRule(
                "domain/LiveFeedCache.kt",
                None,
                "live-feed IO adapter",
                "LiveFeedCache owns SSE and live-feed product downloads.",
            ),
            AllowRule(
                "OfflinePackagesPage.kt",
                None,
                "package sync IO adapter",
                "Offline package downloads are explicit user-triggered sync work.",
            ),
        ),
    ),
)


def iter_kotlin_sources() -> list[Path]:
    return sorted(SRC_ROOT.rglob("*.kt"))


def rel(path: Path) -> str:
    return path.relative_to(SRC_ROOT).as_posix()


def classify(check: Check, rel_path: str, line: str, context_lines: tuple[str, ...]) -> AllowRule | None:
    for rule in check.allow:
        if rule.matches(rel_path, line, context_lines):
            return rule
    return None


def main() -> int:
    violations: list[str] = []
    counts: Counter[tuple[str, str]] = Counter()
    for path in iter_kotlin_sources():
        rel_path = rel(path)
        lines = path.read_text().splitlines()
        for line_no, line in enumerate(lines, start=1):
            context_lines = tuple(lines[max(0, line_no - 8):line_no])
            for check in CHECKS:
                if not check.pattern.search(line):
                    continue
                rule = classify(check, rel_path, line, context_lines)
                if rule is None:
                    violations.append(
                        f"{rel_path}:{line_no}: unclassified {check.name}: {line.strip()}"
                    )
                else:
                    counts[(check.name, rule.classification)] += 1

    if violations:
        print("Android slow UI work audit failed:", file=sys.stderr)
        for violation in violations:
            print(f"  {violation}", file=sys.stderr)
        print(
            "\nClassify legitimate call sites in ui/android-app/scripts/check_slow_ui_calls.py, "
            "or route them through an approved runner/scheduler.",
            file=sys.stderr,
        )
        return 1

    print("Android slow UI work audit passed:")
    for (check_name, classification), count in sorted(counts.items()):
        print(f"  {check_name}: {classification}: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
