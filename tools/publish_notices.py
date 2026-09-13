#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Publish operational bulletins, independently of application releases.

The Rust contract validator is installed once with the service tooling. This
command never compiles or deploys a release. The public file is the authoritative
committed state; each writer changes only the section it owns under the lock.
"""
from __future__ import annotations

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from urllib.request import Request, urlopen

REPO = Path(__file__).resolve().parents[1]
BULLETIN_PATH = "/service/bulletins-v1.json"
MAX_BYTES = 256 * 1024


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def read_document(root: Path) -> dict | None:
    path = root / "bulletins-v1.json"
    if not path.exists():
        return None
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError("installed bulletin exceeds contract size limit")
    return json.loads(data)


def validate(document: dict, validator: Path) -> bytes:
    raw = json.dumps(document, separators=(",", ":")).encode()
    if len(raw) > MAX_BYTES:
        raise ValueError("bulletin exceeds 256 KiB; resolve/archive policy needs operator attention")
    result = subprocess.run([str(validator)], input=raw, capture_output=True, timeout=10)
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return result.stdout


def atomic_write(path: Path, data: bytes, *, http_validator: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    # nginx's static ETag contains second-resolution mtime and length. Ensure
    # equal-length publications cannot share a validator within one second.
    if http_validator and path.exists():
        delay = int(path.stat().st_mtime) + 1 - time.time()
        if delay > 0:
            time.sleep(delay)
    descriptor, temporary = tempfile.mkstemp(prefix=".bulletin-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fchmod(stream.fileno(), 0o644)
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        with contextlib.suppress(FileNotFoundError):
            os.unlink(temporary)


def publish(root: Path, publisher: str, validator: Path, *, notices: list | None = None,
            releases: list | None = None, expected_revision: int | None = None,
            source: str = "operator") -> dict:
    root.mkdir(parents=True, exist_ok=True)
    with (root / "publication.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            receipt = _publish_locked(root, publisher, validator, notices=notices,
                                      releases=releases, expected_revision=expected_revision, source=source)
        except Exception as error:
            write_publication_status(root, None, str(error))
            raise
        write_publication_status(root, receipt, None)
        return receipt


def write_publication_status(root: Path, receipt: dict | None, error: str | None) -> None:
    catalog = REPO / "contracts/telemetry"
    pins = json.loads((catalog / "producers.json").read_text())["producers"]
    pin = pins["service-bulletins"]
    descriptor = catalog / (pin["id"] + ".json")
    if hashlib.sha256(descriptor.read_bytes()).hexdigest() != pin["sha256"]:
        raise ValueError("service bulletin telemetry descriptor mismatch")
    atomic_write(root / "publication-status.json", json.dumps({
        "schema_version": 1, "telemetry_contract": pin, "attempted_at_utc": utc_now(),
        "publication_failure_count": int(error is not None),
        "error": error, "receipt": receipt,
    }, indent=2).encode())


def _publish_locked(root: Path, publisher: str, validator: Path, *, notices: list | None,
                    releases: list | None, expected_revision: int | None, source: str) -> dict:
    previous = read_document(root)
    revision = previous["revision"] if previous else 0
    if expected_revision is not None and revision != expected_revision:
        raise ValueError(f"publication changed since preview ({expected_revision} -> {revision}); preview again")
    if previous:
        validate(previous, validator)
        if previous["publisher"] != publisher:
            raise ValueError("refusing to replace this store's publisher identity")
    document = previous or {
        "schema_version": 1, "publisher": publisher, "revision": 0,
        "published_at_utc": utc_now(), "releases": [], "notices": [],
    }
    document = json.loads(json.dumps(document))
    if notices is not None:
        existing = {notice["id"]: notice for notice in document["notices"]}
        incoming_ids = set()
        for notice in notices:
            ident = notice["id"]
            if ident in incoming_ids:
                raise ValueError(f"duplicate incoming notice {ident}")
            incoming_ids.add(ident)
            old = existing.get(ident)
            if old and notice["attention_revision"] < old["attention_revision"]:
                raise ValueError(f"attention revision cannot decrease: {ident}")
            existing[ident] = notice
        document["notices"] = sorted(existing.values(), key=lambda notice: notice["id"])
    if releases is not None:
        document["releases"] = releases
    if previous and document == previous:
        return {"revision": revision, "publisher": publisher, "changed": False,
                "sha256": hashlib.sha256((root / "bulletins-v1.json").read_bytes()).hexdigest()}
    document["revision"] = revision + 1
    document["published_at_utc"] = utc_now()
    encoded = validate(document, validator)
    digest = hashlib.sha256(encoded).hexdigest()
    audit = {"source": source, "sha256": digest, "document": document}
    # An interrupted publication can leave an uncommitted audit candidate.
    # Only bulletins-v1.json identifies the committed revision.
    atomic_write(root / "history" / f"{revision + 1}-{digest}.json",
                 json.dumps(audit, indent=2).encode())
    atomic_write(root / "bulletins-v1.json", encoded, http_validator=True)
    return {"revision": revision + 1, "publisher": publisher, "sha256": digest, "changed": True}


def publish_releases(root: Path, publisher: str, validator: Path, desired, observed) -> dict:
    """Call after channel activation; retain retired release support records."""
    previous = read_document(root)
    retained = {item["release"]: item for item in (previous or {}).get("releases", [])}
    base = publisher.removesuffix(BULLETIN_PATH)
    deadlines = {item.tag: item.until_utc for item in desired.sunset}
    active = {observed.production: "production"}
    if observed.staging:
        active[observed.staging] = "staging"
    active.update({tag: "sunset" for tag in observed.sunset})
    for tag, role in active.items():
        record = observed.releases[tag]
        deadline = deadlines.get(tag) if role == "sunset" else None
        if role == "sunset" and not deadline:
            raise ValueError(f"activated sunset release {tag} has no authoritative deadline")
        old = retained.get(tag)
        attention = old["attention_revision"] if old else 1
        if old and (old["support_until_utc"], old["role"]) != (deadline, role):
            attention += 1
        retained[tag] = {
            "release": tag, "commit": record.commit, "role": role,
            "support_until_utc": deadline, "attention_revision": attention,
            "web_update_url": base + "/", "android_update_url": base + "/about",
        }
    for tag, item in list(retained.items()):
        if tag not in active:
            # A removed staging candidate without a support promise is not an
            # expired production installation. Do not manufacture a deadline.
            if item["support_until_utc"] is None:
                del retained[tag]
            else:
                item["role"] = "retired"
    return publish(root, publisher, validator, releases=sorted(retained.values(), key=lambda item: item["release"]),
                   source="activated-release-assignments")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    target = parser.add_mutually_exclusive_group()
    target.add_argument("--check", action="store_true")
    target.add_argument("--prod", action="store_true")
    target.add_argument("--dev-stack", action="store_true")
    parser.add_argument("file", nargs="?", type=Path)
    parser.add_argument("--config", type=Path, default=REPO / "deploy/aerobag-prod.json")
    parser.add_argument("--root", type=Path)
    parser.add_argument("--publisher")
    parser.add_argument("--validator", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", "/root/aerobag-artifacts/target")) / "debug/service-bulletin-contract")
    parser.add_argument("--yes", action="store_true")
    parser.add_argument("--inspect", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--apply-stdin", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--expected-revision", type=int)
    args = parser.parse_args()
    if args.apply_stdin:
        payload = json.loads(sys.stdin.buffer.read(MAX_BYTES + 1))
        print(json.dumps(publish(args.root, args.publisher, args.validator,
                                 notices=payload["notices"], expected_revision=args.expected_revision)))
        return 0
    if args.inspect:
        print(json.dumps(read_document(args.root)))
        return 0
    config = json.loads(args.config.read_text()) if args.prod else None
    remote_command = None
    if config:
        args.publisher = config["service_public_base_url"].rstrip("/") + BULLETIN_PATH
        args.root = Path(config["data_root"]) / "service"
        remote_command = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
                          f"{config['ssh_user']}@{config['ssh_host']}"]
        base_command = ["python3", config["source_root"] + "/tools/publish_notices.py",
                        "--root", str(args.root), "--publisher", args.publisher,
                        "--validator", "/usr/local/bin/aerobag-validate-bulletin"]
        result = subprocess.run(remote_command + [shlex.join(base_command + ["--inspect"])],
                                capture_output=True, text=True, check=True, timeout=30)
        previous = json.loads(result.stdout)
    else:
        args.root = args.root or Path("/root/aerobag-artifacts/dev-stack/service")
        args.publisher = args.publisher or "http://aerobag-dev.iac.jonh.net:18080" + BULLETIN_PATH
        previous = read_document(args.root)
    if not args.file:
        parser.error("a notices JSON file is required")
    payload = json.loads(args.file.read_bytes())
    if set(payload) != {"notices"} or not isinstance(payload["notices"], list):
        raise ValueError('expected an object with a "notices" array')
    preview = {"schema_version": 1, "publisher": args.publisher, "revision": 1,
               "published_at_utc": utc_now(), "releases": [], "notices": payload["notices"]}
    validate(preview, args.validator)
    if args.check:
        print("Notice contract valid.")
        return 0
    print(f"Publisher: {args.publisher}\nCurrent revision: {(previous or {}).get('revision', 0)}")
    print(json.dumps(payload, indent=2))
    print("Existing notices omitted here are retained. Set resolved=true to resolve a notice.")
    if not args.yes and input("Publish these changes? [y/N] ").lower() != "y":
        return 1
    expected = (previous or {}).get("revision", 0)
    if remote_command:
        result = subprocess.run(remote_command + [shlex.join(base_command + ["--apply-stdin", "--expected-revision", str(expected)])],
                                input=json.dumps(payload), text=True, capture_output=True, check=True, timeout=30)
        receipt = json.loads(result.stdout)
    else:
        receipt = publish(args.root, args.publisher, args.validator, notices=payload["notices"], expected_revision=expected)
    print(json.dumps(receipt, indent=2))
    try:
        with urlopen(Request(args.publisher, headers={"Cache-Control": "no-cache"}), timeout=15) as response:
            served = response.read(MAX_BYTES + 1)
        if hashlib.sha256(served).hexdigest() != receipt["sha256"]:
            raise ValueError("public digest differs; another publication may have followed this one")
    except Exception as error:
        raise RuntimeError(f"Publication committed at revision {receipt['revision']}, but public verification failed: {error}") from error
    print("Public bulletin verified. No release deployment or daemon restart was performed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Bulletin publication: {error}", file=sys.stderr)
        raise SystemExit(1)
