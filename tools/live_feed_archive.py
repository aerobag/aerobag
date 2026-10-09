# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Copy immutable publication bytes, using existing feed deltas without decoding them.

Only a NOTAM daily baseline needs materialization; the shared Rust engine owns it.
The original version manifest accompanies each event, but payloads contains exactly
the baseline/delta/replacement dependencies retained by this archive event.
"""

from __future__ import annotations

import hashlib
import io
import json
import lzma
from pathlib import Path
import shutil
import subprocess
import zipfile
import zlib

from product_archive import read_json
from live_feed_contract import LIVE_FEEDS_CONTRACT_PATH


SCHEMA = int(LIVE_FEEDS_CONTRACT_PATH.removeprefix("v"))


def member(root, relative):
    relative = Path(relative)
    if relative.is_absolute() or not relative.parts or any(p in {"..", "."} for p in relative.parts):
        raise ValueError(f"unsafe archive member: {relative}")
    path = root / relative
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f"archive member escapes source: {relative}")
    return path


def manifest(root, head):
    value = read_json(member(root, head["manifest"]))
    if value["schema_version"] != SCHEMA or value["version"] != head["version"]:
        raise ValueError("live-feed manifest contract/identity mismatch")
    return value


def copy_payload(root, ref, target):
    source = member(root, ref["url"])
    destination = member(target, ref["url"])
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    with destination.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if destination.stat().st_size != ref["bytes"] or digest != ref["blob_sha256"]:
        raise ValueError(f"archive payload identity mismatch: {ref['url']}")
    if source.name == "manifest.json":
        # Directory products (NEXRAD and NavKv) keep all subordinate files,
        # not just the descriptor or the pages visible in one viewport.
        inventory = sorted(source.parent.rglob("*"))
        for path in inventory:
            if path.is_symlink():
                raise ValueError(f"symlink in immutable live-feed directory: {path}")
            if path.is_file() and path != source:
                output = destination.parent / path.relative_to(source.parent)
                output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, output)
        if inventory != sorted(source.parent.rglob("*")):
            raise OSError("live-feed directory changed during archive copy")
    return dict(ref)


def verified_bytes(root, ref):
    data = member(root, ref["url"]).read_bytes()
    if len(data) != ref["bytes"] or hashlib.sha256(data).hexdigest() != ref["blob_sha256"]:
        raise ValueError(f"archive payload identity mismatch: {ref['url']}")
    return data


def verify_directory(root, value, target):
    # Authenticated package inventories prove that a concurrent producer GC did
    # not remove a page/tile before our directory walk even began. No NavKv
    # parsing, XZ inflation, or second copy of the package is required.
    refs = ([value["install_state"]] if value.get("install_state") else
            list(value.get("install_profiles", {}).values()))
    if not refs:
        raise ValueError("directory publication has no complete package inventory")
    expected = {}
    for ref in refs:
        with zipfile.ZipFile(io.BytesIO(verified_bytes(root, ref))) as package:
            for info in package.infolist():
                if not info.is_dir():
                    identity = (info.file_size, info.CRC)
                    if info.filename in expected and expected[info.filename] != identity:
                        raise ValueError("conflicting directory inventory entries")
                    expected[info.filename] = identity
    directory = member(target, value["state"]["url"]).parent
    actual = {str(path.relative_to(directory)) for path in directory.rglob("*") if path.is_file()}
    required = set(expected)
    if value["product"] == "nexrad":
        # Offline profiles contain resolutions 0 and 1; web also serves the
        # coarser resolutions. The authenticated manifest enumerates all of them.
        descriptor = read_json(directory / "manifest.json")
        required = {"manifest.json"}
        for level in descriptor["levels"]:
            required.update(descriptor["tile_path_template"].format(res=level["res"], x=x, y=y)
                            for x in range(level["tile_cols"]) for y in range(level["tile_rows"]))
    if not required <= actual or not expected.keys() <= required:
        raise ValueError("incomplete directory inventory in archived publication")
    for name, (size, checksum) in expected.items():
        path = member(directory, name)
        crc = 0
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                crc = zlib.crc32(block, crc)
        if path.stat().st_size != size or crc != checksum:
            raise ValueError(f"directory inventory checksum mismatch: {name}")


class LiveSource:
    history_seconds = 34 * 60
    def __init__(self, root, identity, notam_helper):
        self.root, self.identity, self.notam_helper = Path(root), identity, notam_helper

    def heads(self):
        current = read_json(self.root / "current.json")
        if current["schema_version"] != SCHEMA:
            raise ValueError("unsupported live-feed archive schema")
        result = {}
        for product, entry in current["products"].items():
            path = member(self.root, entry["version_manifest_url"])
            result[product] = {
                "version": entry["current"], "manifest": entry["version_manifest_url"],
                "mtime_ns": path.stat().st_mtime_ns,
                "published_at_utc": entry.get("published_at_utc"),
                "collected_at_utc": entry.get("collected_at_utc"),
            }
        return result

    def baseline(self, product, head, target):
        try:
            return self._baseline(product, head, target)
        except FileNotFoundError:
            current = self.heads()[product]
            if current["version"] == head["version"] and current["manifest"] == head["manifest"]:
                raise
            result = self._baseline(product, current, target)
            if current["version"] != head["version"]:
                result["gap"] = f"baseline {head['version']} expired; resumed at {current['version']}"
            return result

    def _baseline(self, product, head, target):
        value = manifest(self.root, head)
        if value["product"] != product:
            raise ValueError("baseline product identity mismatch")
        state = value["state"]
        if state.get("kind") == "notam_checkpoint_xz":
            checkpoint = self._verified_json(state)
            cursor, deltas = checkpoint["state_id"], []
            for ref in value.get("recent_deltas", []):
                if ref["from_version"] == cursor and cursor != head["version"]:
                    deltas.append(self._verified_json(ref))
                    cursor = ref["to_version"]
            if cursor != head["version"]:
                raise ValueError("NOTAM baseline has an incomplete producer delta chain")
            request = {"checkpoint": checkpoint, "deltas": deltas, "target_state_id": head["version"]}
            result = subprocess.run([str(self.notam_helper)], input=json.dumps(request), text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=120)
            encoded = lzma.compress(result.stdout.encode(), preset=6)
            (target / "checkpoint.json.xz").write_bytes(encoded)
            payloads = [{"kind": "notam_checkpoint_xz", "url": "checkpoint.json.xz",
                         "bytes": len(encoded), "blob_sha256": hashlib.sha256(encoded).hexdigest(),
                         "state_sha256": head["version"]}]
        else:
            payloads = [copy_payload(self.root, state, target)]
            if Path(state["url"]).name == "manifest.json":
                verify_directory(self.root, value, target)
        return {"kind": "baseline", "head": head, "manifest": value, "payloads": payloads}

    def _verified_json(self, ref):
        data = verified_bytes(self.root, ref)
        return json.loads(lzma.decompress(data))

    def updates(self, product, previous, head):
        try:
            return self._updates(product, previous, head)
        except FileNotFoundError as error:
            return [{**head, "gap": f"expired delta chain after {previous['version']}: {error}"}]

    def _updates(self, product, previous, head):
        if previous["version"] == head["version"]:
            return []
        current = manifest(self.root, head)
        if current.get("delta_from_previous"):
            # Walk the durable manifest chain, not the coalescing SSE/status stream.
            chain, cursor, seen = [], head, set()
            while cursor["version"] != previous["version"]:
                if cursor["version"] in seen:
                    raise ValueError("cyclic live-feed version chain")
                seen.add(cursor["version"])
                value = manifest(self.root, cursor)
                if value["product"] != product:
                    raise ValueError("live-feed product identity mismatch")
                chain.append(cursor)
                parent = value.get("previous")
                if not parent:
                    return [{**head, "gap": f"producer reset after {previous['version']}"}]
                if parent == previous["version"]:
                    break
                candidates = [member(self.root, f"versions/{product}/{parent}{suffix}.json")
                              for suffix in ("", ".checkpoint")]
                path = next((path for path in candidates if path.exists()), candidates[0])
                cursor = {"version": parent, "manifest": str(path.relative_to(self.root)),
                          "mtime_ns": path.stat().st_mtime_ns}
            return list(reversed(chain))
        # Full-frame products have no parent links. Collect *all* retained frames
        # between the committed cursor and the frozen current manifest.
        candidates = {}
        if head["mtime_ns"] < previous["mtime_ns"]:
            raise ValueError("full-frame publication clock moved backward")
        seen = set(previous.get("same_time_versions", [])) | {previous["version"]}
        for path in member(self.root, f"versions/{product}").glob("*.json"):
            timestamp = path.stat().st_mtime_ns
            if previous["mtime_ns"] <= timestamp <= head["mtime_ns"]:
                value = read_json(path)
                version = value["version"]
                if timestamp == previous["mtime_ns"] and version in seen:
                    continue
                candidates[version] = {"version": version, "manifest": str(path.relative_to(self.root)),
                                       "mtime_ns": timestamp}
        candidates[head["version"]] = head
        updates = sorted(candidates.values(), key=lambda h: (h["mtime_ns"], h["version"] == head["version"], h["version"]))
        timestamp = previous["mtime_ns"]
        for update in updates:
            if update["mtime_ns"] != timestamp:
                seen = set()
                timestamp = update["mtime_ns"]
            seen.add(update["version"])
            update["same_time_versions"] = sorted(seen)
        return updates

    def update(self, product, previous, head, target):
        if head.get("gap"):
            result = self.baseline(product, head, target)
            result["gap"] = head["gap"]
            return result
        value = manifest(self.root, head)
        delta = value.get("delta_from_previous")
        if delta:
            if delta["from_version"] != previous["version"] or delta["to_version"] != head["version"]:
                raise ValueError("archive delta does not join committed state")
            return {"kind": "delta", "head": head, "manifest": value,
                    "payloads": [copy_payload(self.root, delta, target)]}
        result = self.baseline(product, head, target)
        result["kind"] = "replacement"
        return result
