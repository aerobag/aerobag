#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Deployment-owned product archive. Never changes producer retention or source data."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import time

from live_feed_archive import LiveSource, member
from live_feed_contract import LIVE_FEEDS_CONTRACT_PATH
from product_archive import (BorgStore, DailyCapture, atomic_json, buffer_bytes, disk_bytes, durable_tree, locked,
                             now_utc, read_events, read_json, utc)

GIB = 1024 ** 3


def validate_config(config):
    for name in ("archive_total_gib", "archive_cycles_gib", "archive_buffer_gib", "archive_reserve_gib"):
        if type(config.get(name)) is not int or config[name] <= 0:
            raise ValueError(f"{name} must be a positive integer")
    if config["archive_cycles_gib"] >= config["archive_total_gib"]:
        raise ValueError("archive cycle budget must be less than total budget")
    for name in ("archive_root", "artifact_root", "data_root", "source_root"):
        path = Path(config[name])
        if not path.is_absolute() or ".." in path.parts or path == Path("/"):
            raise ValueError(f"{name} must be an absolute non-root path")
    archive, artifacts = (Path(config[key]).resolve() for key in ("archive_root", "artifact_root"))
    if archive.is_relative_to(artifacts) or artifacts.is_relative_to(archive):
        raise ValueError("archive and production artifact trees must be disjoint")


def require_mount(config, *, writing=True):
    root = Path(config["archive_root"])
    if not root.is_mount():
        raise OSError(f"archive filesystem is not mounted: {root}")
    if writing and shutil.disk_usage(root).free < config["archive_reserve_gib"] * GIB:
        raise OSError("archive filesystem free-space reserve reached")
    return root


def production_sources(artifact_root):
    generation = (artifact_root / "channel-current").resolve(strict=True)
    if generation.parent != artifact_root / "channel-generations":
        raise ValueError("invalid production channel generation")
    descriptor = read_json(generation / "generation.json")
    bindings = read_json(generation / "live-feed-bindings.json")["releases"]
    tags = [descriptor["production"], *descriptor.get("sunset", [])]
    providers = set()
    for tag in tags:
        provider = bindings[tag]["provider"]
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,159}", provider):
            raise ValueError("unsafe live-feed provider identity")
        providers.add(provider)
    return generation, sorted(providers)


def telemetry_pin(config):
    catalog = Path(config["source_root"]) / "contracts/telemetry"
    pin = read_json(catalog / "producers.json")["producers"]["product-archive"]
    raw = (catalog / (pin["id"] + ".json")).read_bytes()
    if hashlib.sha256(raw).hexdigest() != pin["sha256"]:
        raise ValueError("archive telemetry descriptor hash mismatch")
    return pin


def capture_once(config, root):
    artifacts = Path(config["artifact_root"])
    _, providers = production_sources(artifacts)
    capture = DailyCapture(root / "live-buffer", max_bytes=config["archive_buffer_gib"] * GIB)
    errors = []
    at = now_utc()
    identities = {provider + "/" + LIVE_FEEDS_CONTRACT_PATH for provider in providers}
    retired = {}
    for pointer in (capture.root / "active").glob("*.json"):
        batch = capture.root / "batches" / read_json(pointer)["batch"]
        identity = read_json(batch / "batch.json")["source"]
        if identity not in identities:
            retired[identity] = batch
    for identity in sorted(identities | retired.keys()):
        provider, contract = identity.split("/")
        if contract != LIVE_FEEDS_CONTRACT_PATH:
            raise ValueError(f"cannot retire unsupported live-feed archive contract: {contract}")
        source = LiveSource(artifacts / "live-feeds/instances" / provider / LIVE_FEEDS_CONTRACT_PATH,
                            identity, config["archive_notam_helper"])
        try:
            capture.collect(source, at)
        except Exception as error:
            errors.append(f"{provider}: {error}")
            if identity in retired:
                capture.record_gap("retired:" + identity, {
                    "at_utc": at, "product": "*", "reason": f"retired provider final capture failed: {error}",
                }, retired[identity])
    # A release's final partial day is a valid independently recoverable unit.
    capture.retire(identities, at)
    if errors:
        raise RuntimeError("; ".join(errors))


def pin_cycle(config):
    artifacts = Path(config["artifact_root"])
    pending = artifacts / "state/archive-cycle-pending"
    # Reuse the controller lock only while obtaining a stable, hard-linked tree.
    # Borg's potentially slow scan/copy runs after releasing it.
    with locked(artifacts / "locks/release-reconciler.lock", blocking=False):
        if (pending / "ready.json").exists():
            return pending
        generation, _ = production_sources(artifacts)
        source = generation / "production/packages/current_artifacts.json"
        raw = source.read_bytes()
        identity = hashlib.sha256(raw).hexdigest()
        done = artifacts / "state/archive-cycle-last.json"
        if done.exists() and read_json(done)["identity"] == identity:
            if pending.exists():
                shutil.rmtree(pending)
            return None
        descriptor = read_json(generation / "generation.json")
        # At most one bounded pending cycle snapshot. Finish it before newer work.
        if pending.exists():
            shutil.rmtree(pending)
        pending.mkdir(parents=True)
        (pending / "current_artifacts.json").write_bytes(raw)
        publications = set()
        for manifest in json.loads(raw):
            for relative in manifest["artifact_roots"].values():
                publications.add(member(artifacts / "published", relative).parent)
        for publication in sorted(publications):
            if not publication.is_dir():
                raise FileNotFoundError(f"missing cycle publication: {publication}")
            destination = pending / "published" / publication.relative_to(artifacts / "published")
            # Published files are immutable. Hardlinks pin bytes without depending
            # on cache identity, and are confined to this owned staging directory.
            for directory, dirs, files in os.walk(publication):
                for name in [*dirs, *files]:
                    if (Path(directory) / name).is_symlink():
                        raise ValueError("symlink in immutable cycle publication")
                output = destination / Path(directory).relative_to(publication)
                output.mkdir(parents=True, exist_ok=True)
                for name in files:
                    os.link(Path(directory) / name, output / name)
        durable_tree(pending)
        atomic_json(pending / "ready.json", {
            "identity": identity, "captured_at_utc": now_utc(), "generation": descriptor,
        })
        return pending


def store_once(config, root):
    reserve = config["archive_reserve_gib"] * GIB
    with locked(root / "store.lock", blocking=False):
        live = BorgStore(root / "live-feeds.borg",
                         max_bytes=(config["archive_total_gib"] - config["archive_cycles_gib"]) * GIB,
                         reserve_bytes=reserve)
        capture = DailyCapture(root / "live-buffer", max_bytes=config["archive_buffer_gib"] * GIB)
        errors = []
        try:
            live.commit_sealed(capture)
        except Exception as error:
            errors.append(f"live-feed archive: {error}")
        cycles = BorgStore(root / "cycles.borg", max_bytes=config["archive_cycles_gib"] * GIB,
                           reserve_bytes=reserve)
        try:
            pending = pin_cycle(config)
            if pending:
                metadata = read_json(pending / "ready.json")
                name = "cycle-" + metadata["captured_at_utc"].replace(":", "") + "-" + metadata["identity"][:16]
                cycles.commit(name, pending)
                atomic_json(Path(config["artifact_root"]) / "state/archive-cycle-last.json", metadata)
                shutil.rmtree(pending)
        except Exception as error:
            errors.append(f"cycle archive: {error}")
        for store in (cycles, live):
            if (store.repository / "config").exists():
                store.prune()
        if errors:
            raise RuntimeError("; ".join(errors))


def status(config, mode, error):
    root = Path(config["archive_root"])
    path = Path(config["data_root"]) / "health/archive" / (mode + ".json")
    old = read_json(path) if path.exists() else {}
    atomic_json(path, {
        "schema_version": 1, "telemetry_contract": telemetry_pin(config),
        "attempted_at_utc": now_utc(), "last_success_at_utc": old.get("last_success_at_utc") if error else now_utc(),
        "failure_count": int(error is not None), "error": str(error) if error else None,
        "archive_root": str(root),
        "repository_bytes": disk_bytes(root / ("live-feeds.borg" if mode == "collect" else "cycles.borg")),
        "buffer_bytes": buffer_bytes(root / "live-buffer/batches"),
        "capture_gap_count": len(list((root / "live-buffer/gaps").glob("*.json"))),
    })


def export_live(batch, product, at, output):
    """Export a baseline and ordered updates from an extracted day, without live inputs."""
    selected = [row for row in read_events(batch) if row["product"] == product
                and utc(row["effective_at_utc"]) <= utc(at)]
    if not selected:
        raise ValueError("no captured state at or before the requested time")
    baseline = max(i for i, row in enumerate(selected) if row["kind"] in {"baseline", "replacement"})
    selected = selected[baseline:]
    output.mkdir(parents=True, exist_ok=False)
    for row in selected:
        shutil.copytree(member(batch, row["directory"]), member(output, row["directory"]))
    gaps = [read_json(path) for path in (batch / "gaps").glob("*.json")]
    atomic_json(output / "timeline.json", {"schema_version": 1, "product": product, "events": selected, "gaps": gaps})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=Path("/etc/aerobag/archive.json"))
    parser.add_argument("--mode", choices=("collect", "store"))
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--acknowledge-gaps", action="store_true",
                        help="Clear latched capture-gap alarms after reviewing live-buffer/gaps")
    parser.add_argument("--export-live", type=Path, metavar="EXTRACTED_DAY")
    parser.add_argument("--product")
    parser.add_argument("--at")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.export_live:
        if not all((args.product, args.at, args.output)):
            parser.error("export needs --product, --at, and --output")
        export_live(args.export_live, args.product, args.at, args.output)
        return
    config = read_json(args.config)
    validate_config(config)
    if args.acknowledge_gaps:
        root = require_mount(config, writing=False) / "live-buffer"
        with locked(root / "collector.lock"):
            for path in (root / "gaps").glob("*.json"):
                path.unlink()
        return
    if not args.mode:
        parser.error("--mode is required")
    while True:
        error = None
        try:
            root = require_mount(config, writing=args.mode == "collect")
            (capture_once if args.mode == "collect" else store_once)(config, root)
        except Exception as caught:
            error = caught
            print(f"archive {args.mode}: {caught}", file=sys.stderr, flush=True)
        status(config, args.mode, error)
        if args.once or args.mode == "store":
            if error:
                raise SystemExit(1)
            return
        time.sleep(15)


if __name__ == "__main__":
    main()
