#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import zipfile
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


DEFAULT_ARTIFACT_ROOT = Path("/root/aerobag-artifacts")


@dataclass(frozen=True)
class PlateRecord:
    airport_id: str
    package_id: str
    label: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Audit how well published TPP approach plates line up with CIFP approach "
            "records in main.db."
        )
    )
    parser.add_argument(
        "--artifact-root",
        type=Path,
        default=DEFAULT_ARTIFACT_ROOT,
        help=f"Artifact root to inspect (default: {DEFAULT_ARTIFACT_ROOT})",
    )
    parser.add_argument(
        "--bundle",
        type=Path,
        help="Explicit bundle_XXXX.json to inspect. Defaults to the highest production bundle.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=20,
        help="How many example rows to show in each summary section.",
    )
    return parser.parse_args()


def choose_bundle(artifact_root: Path, explicit_bundle: Path | None) -> Path:
    if explicit_bundle is not None:
        return explicit_bundle
    production_root = artifact_root / "published-packaged" / "production"
    bundles = sorted(production_root.glob("bundle_*.json"))
    if not bundles:
        raise SystemExit(f"no bundle_*.json files found under {production_root}")
    return bundles[-1]


def load_bundle(bundle_path: Path) -> dict:
    return json.loads(bundle_path.read_text())


def resolve_db_path(artifact_root: Path, bundle: dict) -> Path:
    cycle = bundle["cycle"]
    relative_zip = Path(bundle["data"]["relative_path"])
    unpacked_dir = artifact_root / "published-unpacked" / "production" / cycle / relative_zip.with_suffix("")
    candidates = [
        unpacked_dir / "main.db",
        unpacked_dir.parent / "main.db",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise SystemExit(
        "could not locate main.db for bundle "
        f"{cycle}; tried: {', '.join(str(path) for path in candidates)}"
    )


def load_tpp_approach_plates(artifact_root: Path, bundle: dict) -> list[PlateRecord]:
    plates: list[PlateRecord] = []
    for package in bundle["packages"]:
        if package.get("family_id") != "tpp":
            continue
        zip_path = artifact_root / package["relative_path"]
        with zipfile.ZipFile(zip_path) as zf:
            manifest = json.loads(zf.read("package-assets.json"))
        for asset in manifest["assets"]:
            if asset.get("document_type") != "approach":
                continue
            plates.append(
                PlateRecord(
                    airport_id=asset["airport_id"].strip(),
                    package_id=manifest["package_id"],
                    label=asset["label"].strip(),
                )
            )
    return plates


def load_cifp_approaches(db_path: Path) -> dict[str, set[str]]:
    conn = sqlite3.connect(db_path)
    try:
        cur = conn.cursor()
        rows = cur.execute(
            """
            SELECT DISTINCT
              trim(airport_identifier),
              trim(sid_star_approach_identifier)
            FROM cifp_sid_star_app
            WHERE trim(route_type) NOT IN ('1', '2', '3', '4', '5', '6', 'T')
            """
        )
        by_airport: dict[str, set[str]] = defaultdict(set)
        for airport_id, procedure_id in rows:
            if airport_id and procedure_id:
                by_airport[airport_id].add(procedure_id)
        return by_airport
    finally:
        conn.close()


def load_airport_aliases(db_path: Path) -> dict[str, str]:
    conn = sqlite3.connect(db_path)
    try:
        cur = conn.cursor()
        rows = cur.execute(
            "SELECT trim(alias_id), trim(airport_id) FROM airport_aliases"
        )
        aliases: dict[str, str] = {}
        for alias_id, airport_id in rows:
            if alias_id and airport_id:
                aliases[alias_id] = airport_id
        return aliases
    finally:
        conn.close()


def strip_iap_prefix(label: str) -> str:
    match = re.match(r"^IAP-[A-Z]{2}-(.+)$", label)
    return match.group(1) if match else label


def runway_candidate(prefix: str, runway: str, variant: str | None, *, style: str = "hyphen") -> str:
    if not variant:
        return f"{prefix}{runway}"
    if style == "auto":
        style = "suffix" if runway.endswith(("L", "R", "C")) else "hyphen"
    if style == "suffix":
        return f"{prefix}{runway}{variant}"
    return f"{prefix}{runway}-{variant}"


def circling_candidate(prefix: str, variant: str) -> str:
    return f"{prefix}-{variant}"


def expand_runway_pair(text: str) -> list[str]:
    match = re.fullmatch(r"([0-9]{1,2})([LRC]?)(?: AND )([LRC])", text)
    if not match:
        return [text]
    base, first_suffix, second_suffix = match.groups()
    runways = [f"{base}{first_suffix}"]
    second = f"{base}{second_suffix}"
    if second not in runways:
        runways.append(second)
    return runways


def heuristic_candidate_groups(label: str) -> list[set[str]]:
    body = strip_iap_prefix(label.upper()).strip()
    body = re.sub(r" \((?:SA )?CAT[^)]*\)$", "", body)
    groups: list[set[str]] = []

    patterns: list[tuple[re.Pattern[str], callable]] = [
        (
            re.compile(r"^VOR(?: AND DME|/DME) OR TACAN RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("V", m.group(1), None)},
                {runway_candidate("T", m.group(1), None)},
                {runway_candidate("S", m.group(1), None)},
                {runway_candidate("D", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^VOR(?: AND DME|/DME) OR TACAN RWY ([0-9]{1,2}[LRC]? AND [LRC])$"),
            lambda m: [
                {runway_candidate("V", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("T", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("S", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("D", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ],
        ),
        (
            re.compile(r"^ILS(?: ([XYZ]))? OR LOC(?: OR DME|/DME)?(?: \1)? RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("I", m.group(2), m.group(1), style="auto")},
                {runway_candidate("L", m.group(2), m.group(1), style="auto")},
            ],
        ),
        (
            re.compile(r"^ILS PRM RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("I", m.group(1), None)},
                {runway_candidate("L", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^ILS PRM ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("I", m.group(2), m.group(1), style="auto")},
                {runway_candidate("L", m.group(2), None)},
            ],
        ),
        (
            re.compile(r"^ILS(?: ([XYZ]))? OR LOC AND DME(?: \1)? RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("I", m.group(2), m.group(1), style="auto")},
                {runway_candidate("L", m.group(2), m.group(1), style="auto")},
            ],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)? OR TACAN RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("V", m.group(1), None)},
                {runway_candidate("T", m.group(1), None)},
                {runway_candidate("S", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)? OR TACAN RWY ([0-9]{1,2}[LRC]? AND [LRC])$"),
            lambda m: [
                {runway_candidate("V", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("T", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("S", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ],
        ),
        (
            re.compile(r"^LOC(?:/DME)? RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("L", m.group(1), None)}],
        ),
        (
            re.compile(r"^LOC AND DME RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("L", m.group(1), None)}],
        ),
        (
            re.compile(r"^LOC(?:/DME)? ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("L", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^LOC AND DME ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("L", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^ILS RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("I", m.group(1), None)}],
        ),
        (
            re.compile(r"^ILS ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("I", m.group(2), m.group(1), style="auto")},
                {runway_candidate("L", m.group(2), m.group(1), style="auto")},
            ],
        ),
        (
            re.compile(r"^RNAV \(GPS\) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("R", m.group(1), None)}],
        ),
        (
            re.compile(r"^GPS RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("P", m.group(1), None)}],
        ),
        (
            re.compile(r"^RNAV \(GPS\) ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("R", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^RNAV \(RNP\) ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("H", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^RNAV \(RNP\) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("H", m.group(1), None)}],
        ),
        (
            re.compile(r"^RNAV \(RNP\) RWY ([0-9]{1,2}[LRC]? AND [LRC])$"),
            lambda m: [
                {runway_candidate("H", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ],
        ),
        (
            re.compile(r"^GLS RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("G", m.group(1), None)}],
        ),
        (
            re.compile(r"^SDF RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("S", m.group(1), None)}],
        ),
        (
            re.compile(r"^SDF ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("S", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^NDB(?:/DME)? RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("N", m.group(1), None)},
                {runway_candidate("S", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^NDB(?:/DME)? ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("N", m.group(2), m.group(1), style="auto")},
                {runway_candidate("S", m.group(2), m.group(1), style="auto")},
            ],
        ),
        (
            re.compile(r"^VOR(?: AND DME|/DME) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("V", m.group(1), None)},
                {runway_candidate("S", m.group(1), None)},
                {runway_candidate("D", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^VOR(?: AND DME|/DME) RWY ([0-9]{1,2}[LRC]? AND [LRC])$"),
            lambda m: [
                {runway_candidate("V", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("S", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("D", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)? RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("V", m.group(1), None)},
                {runway_candidate("S", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)? RWY ([0-9]{1,2}[LRC]? AND [LRC])$"),
            lambda m: [
                {runway_candidate("V", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ]
            + [
                {runway_candidate("S", runway, None)}
                for runway in expand_runway_pair(m.group(1))
            ],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)? ([UVWXYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("V", m.group(2), m.group(1), style="auto")},
                {runway_candidate("S", m.group(2), m.group(1), style="auto")},
            ],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)? ([UVWXYZ]) OR TACAN(?: \1)? RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("V", m.group(2), m.group(1), style="auto")},
                {runway_candidate("T", m.group(2), m.group(1), style="auto")},
                {runway_candidate("S", m.group(2), m.group(1), style="auto")},
            ],
        ),
        (
            re.compile(r"^TACAN RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [
                {runway_candidate("T", m.group(1), None)},
                {runway_candidate("S", m.group(1), None)},
            ],
        ),
        (
            re.compile(r"^TACAN ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("T", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^HI-TACAN RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("H", m.group(1), None)}],
        ),
        (
            re.compile(r"^HI-TACAN ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("H", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^VOR(?: OR DME|/DME)?-([A-Z])$"),
            lambda m: [{circling_candidate("VOR", m.group(1)), circling_candidate("VDM", m.group(1))}],
        ),
        (
            re.compile(r"^VOR OR TACAN-([A-Z])$"),
            lambda m: [{circling_candidate("VOR", m.group(1))}],
        ),
        (
            re.compile(r"^VOR OR GPS-([A-Z])$"),
            lambda m: [{circling_candidate("VOR", m.group(1))}],
        ),
        (
            re.compile(r"^VOR(?: AND DME|/DME)-([A-Z])$"),
            lambda m: [{circling_candidate("VDM", m.group(1))}],
        ),
        (
            re.compile(r"^VOR AND DME OR GPS-([A-Z])$"),
            lambda m: [{circling_candidate("VDM", m.group(1))}],
        ),
        (
            re.compile(r"^NDB(?:/DME)?-([A-Z])$"),
            lambda m: [{circling_candidate("NDB", m.group(1))}],
        ),
        (
            re.compile(r"^RNAV \((?:GPS|RNP)\)-([A-Z])$"),
            lambda m: [{circling_candidate("RNV", m.group(1))}],
        ),
        (
            re.compile(r"^LOC(?: AND DME)?-([A-Z])$"),
            lambda m: [{circling_candidate("LOC", m.group(1)), circling_candidate("LDA", m.group(1))}],
        ),
        (
            re.compile(r"^LDA-([A-Z])$"),
            lambda m: [{circling_candidate("LDA", m.group(1))}],
        ),
        (
            re.compile(r"^LDA ([XYZ]) RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("X", m.group(2), m.group(1), style="auto")}],
        ),
        (
            re.compile(r"^LDA RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("X", m.group(1), None)}],
        ),
        (
            re.compile(r"^LOC BC RWY ([0-9]{1,2}[LRC]?)$"),
            lambda m: [{runway_candidate("B", m.group(1), None)}],
        ),
    ]

    for pattern, builder in patterns:
        match = pattern.match(body)
        if match:
            return [
                {candidate.strip() for candidate in group if candidate.strip()}
                for group in builder(match)
                if group
            ]

    return groups


def heuristic_candidates(label: str) -> set[str]:
    return {
        candidate
        for group in heuristic_candidate_groups(label)
        for candidate in group
    }


def compare_counts(
    plates: Iterable[PlateRecord], cifp: dict[str, set[str]], aliases: dict[str, str]
) -> list[tuple[str, int, int]]:
    plate_counts: Counter[str] = Counter()
    for plate in plates:
        plate_counts[aliases.get(plate.airport_id, plate.airport_id)] += 1

    airports = sorted(set(plate_counts) | set(cifp))
    return [
        (airport_id, plate_counts.get(airport_id, 0), len(cifp.get(airport_id, set())))
        for airport_id in airports
    ]


def analyze_matches(
    plates: Iterable[PlateRecord],
    cifp: dict[str, set[str]],
    aliases: dict[str, str],
) -> tuple[Counter[str], list[tuple[PlateRecord, list[set[str]], set[str]]]]:
    summary: Counter[str] = Counter()
    examples: list[tuple[PlateRecord, list[set[str]], set[str]]] = []

    for plate in plates:
        canonical_airport_id = aliases.get(plate.airport_id, plate.airport_id)
        procedure_ids = cifp.get(canonical_airport_id, set())
        candidate_groups = heuristic_candidate_groups(plate.label)
        matched_groups = [group & procedure_ids for group in candidate_groups]
        matched = {candidate for group in matched_groups for candidate in group}
        if not procedure_ids:
            summary["airport_missing_from_cifp"] += 1
            if len(examples) < 50:
                examples.append((plate, candidate_groups, matched))
            continue
        if not candidate_groups:
            summary["no_heuristic"] += 1
            if len(examples) < 50:
                examples.append((plate, candidate_groups, matched))
            continue

        ambiguous_groups = [group for group in matched_groups if len(group) > 1]
        missing_groups = [group for group in matched_groups if len(group) == 0]
        singleton_groups = [group for group in matched_groups if len(group) == 1]

        if ambiguous_groups:
            summary["matched_ambiguous"] += 1
            if len(examples) < 50:
                examples.append((plate, candidate_groups, matched))
            continue
        if missing_groups and not singleton_groups:
            summary["matched_none"] += 1
            if len(examples) < 50:
                examples.append((plate, candidate_groups, matched))
            continue

        if missing_groups:
            summary["matched_partial"] += 1
            if len(examples) < 50:
                examples.append((plate, candidate_groups, matched))
            continue

        summary["matched_unique"] += 1

    return summary, examples


def is_public_plate(label: str) -> bool:
    upper = label.upper()
    return "SA CAT" not in upper and "SPECIAL AIRCREW" not in upper and "COPTER" not in upper


def heuristic_candidate_groups_for_copter_plate(label: str) -> list[set[str]]:
    return heuristic_candidate_groups(
        re.sub(r"^(IAP-[A-Z]{2}-)COPTER ", r"\1", label, flags=re.IGNORECASE)
    )


def classify_relation(
    plates: Iterable[PlateRecord],
    cifp: dict[str, set[str]],
    aliases: dict[str, str],
) -> tuple[Counter[str], list[dict]]:
    plates_by_airport: dict[str, list[PlateRecord]] = defaultdict(list)
    for plate in plates:
        if "VISUAL" in plate.label.upper():
            continue
        canonical_airport_id = aliases.get(plate.airport_id, plate.airport_id)
        if not cifp.get(canonical_airport_id):
            continue
        plates_by_airport[canonical_airport_id].append(plate)

    summary: Counter[str] = Counter()
    examples: list[dict] = []

    for airport_id, airport_plates in sorted(plates_by_airport.items()):
        procedure_ids = set(cifp[airport_id])
        cid_claimers: dict[str, list[dict[str, object]]] = defaultdict(list)
        copter_claimers: dict[str, list[str]] = defaultdict(list)
        ignored_noheur = 0
        ignored_nomatch = 0

        for plate in airport_plates:
            groups = heuristic_candidate_groups(plate.label)
            if "COPTER" in plate.label.upper():
                for group in heuristic_candidate_groups_for_copter_plate(plate.label):
                    matched = group & procedure_ids
                    if len(matched) == 1:
                        cid = next(iter(matched))
                        copter_claimers[cid].append(plate.label)
            if not groups:
                ignored_noheur += 1
                continue

            any_group_bound = False
            for group in groups:
                matched = group & procedure_ids
                if len(matched) == 1:
                    cid = next(iter(matched))
                    cid_claimers[cid].append(
                        {
                            "label": plate.label,
                            "public": is_public_plate(plate.label),
                        }
                    )
                    any_group_bound = True

            if not any_group_bound:
                ignored_nomatch += 1

        uniquely_bound = {
            cid: claimers[0]
            for cid, claimers in cid_claimers.items()
            if len(claimers) == 1
        }
        multiply_bound = {
            cid: claimers
            for cid, claimers in cid_claimers.items()
            if len(claimers) > 1
        }
        copter_only = sorted(
            cid
            for cid in procedure_ids - set(uniquely_bound) - set(multiply_bound)
            if cid in copter_claimers
        )
        unresolved = sorted(
            procedure_ids - set(uniquely_bound) - set(multiply_bound) - set(copter_only)
        )

        summary["airports_considered"] += 1
        summary["uniquely_bound_cids_total"] += len(uniquely_bound)
        summary["multiply_bound_cids_total"] += len(multiply_bound)
        summary["copter_only_cids_total"] += len(copter_only)
        summary["unresolved_cids_total"] += len(unresolved)
        summary["ignored_noheur_plates_total"] += ignored_noheur
        summary["ignored_nomatch_plates_total"] += ignored_nomatch

        if unresolved:
            summary["airports_with_unresolved_cids"] += 1
        else:
            summary["airports_with_no_unresolved_cids"] += 1

        if unresolved or multiply_bound:
            examples.append(
                {
                    "airport": airport_id,
                    "cifp_total": len(procedure_ids),
                    "uniquely_bound": len(uniquely_bound),
                    "multiply_bound": len(multiply_bound),
                    "copter_only": len(copter_only),
                    "unresolved_count": len(unresolved),
                    "copter_only_cids": copter_only,
                    "unresolved_cids": unresolved,
                    "multiply_bound_examples": multiply_bound,
                    "ignored_noheur_plates": ignored_noheur,
                    "ignored_nomatch_plates": ignored_nomatch,
                }
            )

    return summary, examples


def main() -> None:
    args = parse_args()
    bundle_path = choose_bundle(args.artifact_root, args.bundle)
    bundle = load_bundle(bundle_path)
    db_path = resolve_db_path(args.artifact_root, bundle)
    plates = load_tpp_approach_plates(args.artifact_root, bundle)
    cifp = load_cifp_approaches(db_path)
    aliases = load_airport_aliases(db_path)

    count_rows = compare_counts(plates, cifp, aliases)
    mismatches = [row for row in count_rows if row[1] != row[2]]

    print(f"bundle: {bundle_path}")
    print(f"cycle: {bundle['cycle']}")
    print(f"db: {db_path}")
    print(f"approach plates: {len(plates)}")
    print(f"airports with approach plates: {sum(1 for _, plate_count, _ in count_rows if plate_count)}")
    print(f"airports with CIFP approaches: {sum(1 for _, _, cifp_count in count_rows if cifp_count)}")
    print()

    print("count audit:")
    print(f"  airports checked: {len(count_rows)}")
    print(f"  exact count match: {len(count_rows) - len(mismatches)}")
    print(f"  count mismatch: {len(mismatches)}")
    for airport_id, plate_count, cifp_count in sorted(
        mismatches,
        key=lambda row: (abs(row[1] - row[2]), row[0]),
        reverse=True,
    )[: args.limit]:
        print(f"  {airport_id}: plates={plate_count} cifp_iaps={cifp_count}")
    print()

    summary, examples = analyze_matches(plates, cifp, aliases)
    print("heuristic match audit:")
    for key in [
        "matched_unique",
        "matched_partial",
        "matched_none",
        "no_heuristic",
        "airport_missing_from_cifp",
    ]:
        print(f"  {key}: {summary.get(key, 0)}")
    print()

    relation_summary, relation_examples = classify_relation(plates, cifp, aliases)
    print("relation audit:")
    for key in [
        "airports_considered",
        "airports_with_no_unresolved_cids",
        "airports_with_unresolved_cids",
        "uniquely_bound_cids_total",
        "multiply_bound_cids_total",
        "copter_only_cids_total",
        "unresolved_cids_total",
        "ignored_noheur_plates_total",
        "ignored_nomatch_plates_total",
    ]:
        print(f"  {key}: {relation_summary.get(key, 0)}")
    print()

    print("sample difficult cases:")
    shown = 0
    for plate, candidates, matched in examples:
        if shown >= args.limit:
            break
        print(
            f"  {plate.airport_id} {plate.label} "
            f"candidate_groups={sorted(sorted(group) for group in candidates)} matched={sorted(matched)}"
        )
        shown += 1

    print()
    print("sample relation exceptions:")
    for row in sorted(
        relation_examples,
        key=lambda item: (
            item["unresolved_count"],
            item["multiply_bound"],
            item["airport"],
        ),
        reverse=True,
    )[: args.limit]:
        print(
            "  "
            f"{row['airport']} "
            f"unresolved={row['unresolved_count']} "
            f"multiply_bound={row['multiply_bound']} "
            f"unresolved_cids={row['unresolved_cids']}"
        )


if __name__ == "__main__":
    main()
