# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Durable daily capture buffers and Borg storage, independent of feed codecs."""

from __future__ import annotations

from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import uuid


def utc(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.utcoffset() is None:
        raise ValueError("archive timestamps must include a time zone")
    return result.astimezone(timezone.utc)


def now_utc():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def fsync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}")
    try:
        with temporary.open("x") as stream:
            json.dump(value, stream, sort_keys=True, separators=(",", ":"))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        fsync_dir(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def read_json(path):
    return json.loads(path.read_text())


@contextmanager
def locked(path, *, blocking=True):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB))
        yield


def disk_bytes(root):
    seen, total = set(), 0
    for directory, _, files in os.walk(root):
        for name in files:
            try:
                stat = (Path(directory) / name).lstat()
            except FileNotFoundError:
                continue  # The independent Borg worker can finish a sealed buffer.
            identity = (stat.st_dev, stat.st_ino)
            if identity not in seen:
                seen.add(identity)
                total += stat.st_blocks * 512
    return total


def durable_tree(root):
    for directory, _, files in os.walk(root, topdown=False):
        for name in files:
            with (Path(directory) / name).open("rb") as stream:
                os.fsync(stream.fileno())
        fsync_dir(directory)


def database(batch):
    connection = sqlite3.connect(batch / "index.sqlite")
    connection.execute("PRAGMA synchronous=FULL")
    connection.execute("CREATE TABLE IF NOT EXISTS events (sequence INTEGER PRIMARY KEY, event TEXT NOT NULL)")
    connection.execute("CREATE TABLE IF NOT EXISTS heads (product TEXT PRIMARY KEY, head TEXT NOT NULL)")
    connection.execute("CREATE TABLE IF NOT EXISTS usage (id INTEGER PRIMARY KEY CHECK(id=1), bytes INTEGER NOT NULL)")
    connection.execute("INSERT OR IGNORE INTO usage VALUES (1, 0)")
    connection.commit()
    return connection


def buffer_bytes(batches):
    """Payload accounting is transactional; never walk a whole day of tiles per poll."""
    total = 0
    for batch in batches.glob("*"):
        index = batch / "index.sqlite"
        try:
            connection = sqlite3.connect(f"file:{index}?mode=ro", uri=True)
        except sqlite3.OperationalError:
            if not batch.exists():
                continue
            # An unpublished directory interrupted before its index was created.
            total += disk_bytes(batch)
            continue
        try:
            total += connection.execute("SELECT bytes FROM usage WHERE id=1").fetchone()[0]
            try:
                total += index.stat().st_blocks * 512
            except FileNotFoundError:
                pass
        finally:
            connection.close()
    return total


def read_events(batch):
    connection = sqlite3.connect(f"file:{batch / 'index.sqlite'}?mode=ro", uri=True)
    try:
        return [json.loads(row[0]) for row in connection.execute("SELECT event FROM events ORDER BY sequence")]
    finally:
        connection.close()


def batch_heads(batch):
    connection = database(batch)
    try:
        return {product: json.loads(head) for product, head in connection.execute("SELECT product, head FROM heads")}
    finally:
        connection.close()


class DailyCapture:
    def __init__(self, root, *, max_bytes):
        self.root, self.max_bytes = Path(root), max_bytes
        self.checkpoint = lambda point: None
        (self.root / "batches").mkdir(parents=True, exist_ok=True)

    def pointer(self, identity):
        return self.root / "active" / (hashlib.sha256(identity.encode()).hexdigest() + ".json")

    def active(self, identity):
        pointer = self.pointer(identity)
        return self.root / "batches" / read_json(pointer)["batch"] if pointer.exists() else None

    def record_gap(self, identity, value, batch=None):
        name = hashlib.sha256(identity.encode()).hexdigest() + ".json"
        atomic_json(self.root / "gaps" / name, value)
        if batch:
            atomic_json(batch / "gaps" / name, value)

    def _append(self, batch, product, make_event, at):
        if (batch / "sealed.json").exists():
            raise ValueError("cannot append to a sealed archive buffer")
        directory = "events/" + uuid.uuid4().hex
        target = batch / directory
        target.mkdir(parents=True)
        committed = False
        try:
            event = make_event(target)
            event.update(product=product, captured_at_utc=at, directory=directory)
            timestamp = event["head"].get("mtime_ns")
            event["effective_at_utc"] = (datetime.fromtimestamp(timestamp / 1e9, timezone.utc).isoformat()
                                         if timestamp and (event["kind"] != "baseline" or event.get("gap")) else at)
            payload_bytes = disk_bytes(target)
            if buffer_bytes(self.root / "batches") + payload_bytes > self.max_bytes:
                raise OSError("archive buffer quota exceeded; unarchived data retained")
            durable_tree(target)
            fsync_dir(target.parent)
            fsync_dir(batch)
            self.checkpoint("payload_durable")
            # Write the alarm first. A crash must not commit a discontinuity
            # without retaining its health signal.
            if event.get("gap"):
                self.record_gap(f"{batch.name}:{product}:{event['head']['version']}",
                                {"at_utc": at, "product": product, "reason": event["gap"]}, batch)
            connection = database(batch)
            try:
                with connection:
                    connection.execute("INSERT INTO events(event) VALUES (?)", (json.dumps(event),))
                    connection.execute("INSERT OR REPLACE INTO heads VALUES (?, ?)",
                                       (product, json.dumps(event["head"])))
                    connection.execute("UPDATE usage SET bytes=bytes+? WHERE id=1", (payload_bytes,))
            finally:
                connection.close()
            committed = True
            self.checkpoint("event_committed")
            return event["head"]
        finally:
            if not committed:
                shutil.rmtree(target)

    def _recover(self, identity):
        active = self.active(identity)
        metadata = read_json(active / "batch.json") if active else {}
        previous = metadata.get("previous")
        if previous:
            old = self.root / "batches" / previous
            if old.exists() and not (old / "sealed.json").exists():
                atomic_json(old / "sealed.json", {"closed_at_utc": metadata["started_at_utc"]})
        # A crash before switching the pointer leaves only unpublished baselines.
        for batch in (self.root / "batches").iterdir():
            try:
                descriptor = read_json(batch / "batch.json")
            except FileNotFoundError:
                if batch.exists() and not (batch / "sealed.json").exists():
                    shutil.rmtree(batch)
                continue
            if (batch != active and descriptor.get("source") == identity
                    and not (batch / "sealed.json").exists()):
                shutil.rmtree(batch)
        if active:
            committed = {event["directory"] for event in read_events(active)}
            for path in (active / "events").glob("*"):
                if str(path.relative_to(active)) not in committed:
                    shutil.rmtree(path)

    def collect(self, source, at):
        with locked(self.root / "collector.lock"):
            self._recover(source.identity)
            batch = self.active(source.identity)
            day = utc(at).date().isoformat()
            if batch:
                metadata = read_json(batch / "batch.json")
                if day < metadata["day"]:
                    raise ValueError("archive clock moved backward across a day boundary")
                progress = batch / "progress.json"
                if progress.exists() and hasattr(source, "history_seconds"):
                    last = read_json(progress)["collected_at_utc"]
                    elapsed = (utc(at) - utc(last)).total_seconds()
                    if elapsed > source.history_seconds:
                        self.record_gap(f"{source.identity}:{last}", {
                            "at_utc": at, "product": "*",
                            "reason": f"collector absent for {elapsed:.0f}s; full-frame history may have expired",
                        }, batch)
            if batch is None or read_json(batch / "batch.json")["day"] != day:
                previous = batch
                start = day + "T00:00:00Z" if previous else at
                if previous:
                    # Drain late-observed publications from yesterday before
                    # constructing today's independent baseline.
                    frozen = source.heads()
                    for product, cursor in batch_heads(previous).items():
                        if product not in frozen:
                            continue
                        for update in source.updates(product, cursor, frozen[product]):
                            if update.get("mtime_ns", float("inf")) >= utc(start).timestamp() * 1e9:
                                break
                            cursor = self._append(previous, product,
                                                  lambda target, p=product, c=cursor, h=update: source.update(p, c, h, target), at)
                seeds = batch_heads(previous) if previous else source.heads()
                batch = self.root / "batches" / (day + "-" + uuid.uuid4().hex)
                batch.mkdir()
                atomic_json(batch / "batch.json", {
                    "schema_version": 1, "source": source.identity, "day": day,
                    "started_at_utc": start, "previous": previous.name if previous else None,
                })
                database(batch).close()
                try:
                    for product, head in sorted(seeds.items()):
                        self._append(batch, product,
                                     lambda target, p=product, h=head: source.baseline(p, h, target), start)
                    self.checkpoint("baselines_durable")
                    fsync_dir(batch)
                    fsync_dir(batch.parent)
                    atomic_json(self.pointer(source.identity), {"batch": batch.name})
                except BaseException:
                    # replace() may have succeeded before directory fsync failed.
                    # Never delete the target of a committed pointer.
                    if self.active(source.identity) != batch:
                        shutil.rmtree(batch)
                    raise
                self.checkpoint("pointer_committed")
                if previous:
                    atomic_json(previous / "sealed.json", {"closed_at_utc": start})
            # Re-read after baseline capture: publication may have advanced meanwhile.
            heads = batch_heads(batch)
            progress = batch / "progress.json"
            for product, head in sorted(source.heads().items()):
                previous = heads.get(product)
                if previous is None:
                    self._append(batch, product, lambda target: source.baseline(product, head, target), at)
                else:
                    for update in source.updates(product, previous, head):
                        previous = self._append(batch, product,
                                                lambda target, p=previous, h=update: source.update(product, p, h, target), at)
            atomic_json(batch / "progress.json", {"collected_at_utc": at})
            return batch

    def retire(self, identities, at):
        with locked(self.root / "collector.lock"):
            for pointer in (self.root / "active").glob("*.json"):
                batch = self.root / "batches" / read_json(pointer)["batch"]
                identity = read_json(batch / "batch.json")["source"]
                if identity not in identities:
                    self._recover(identity)
                    atomic_json(batch / "sealed.json", {"closed_at_utc": at})
                    pointer.unlink()
                    fsync_dir(pointer.parent)


class BorgStore:
    def __init__(self, repository, *, max_bytes, reserve_bytes, run=subprocess.run):
        self.repository = Path(repository)
        self.max_bytes, self.reserve_bytes, self.run = max_bytes, reserve_bytes, run

    def command(self, *args, **kwargs):
        kwargs.setdefault("env", dict(os.environ, BORG_PASSPHRASE=""))
        return self.run(["borg", *map(str, args)], check=True, text=True,
                        stdout=subprocess.PIPE, timeout=7200, **kwargs).stdout

    def initialize(self):
        if not (self.repository / "config").exists():
            self.command("init", "--encryption=repokey", self.repository,
                         env=dict(os.environ, BORG_PASSPHRASE=""))

    def archives(self):
        return json.loads(self.command("list", "--json", self.repository))["archives"]

    def prune(self):
        archives = sorted(self.archives(), key=lambda row: (row["start"], row["name"]))
        if disk_bytes(self.repository) <= self.max_bytes and shutil.disk_usage(self.repository.parent).free >= self.reserve_bytes:
            return
        target = int(self.max_bytes * .90)
        while (disk_bytes(self.repository) > target or shutil.disk_usage(self.repository.parent).free < self.reserve_bytes) and len(archives) > 1:
            oldest = archives.pop(0)
            self.command("delete", f"{self.repository}::{oldest['name']}")
            self.command("compact", "--threshold=1", self.repository)
        if disk_bytes(self.repository) > self.max_bytes:
            raise OSError("newest complete archive exceeds repository budget")

    def commit(self, name, tree):
        self.initialize()
        self.prune()
        if shutil.disk_usage(self.repository.parent).free < self.reserve_bytes:
            raise OSError("archive filesystem free-space reserve reached")
        destination = f"{self.repository}::{name}"
        if name in {item["name"] for item in self.archives()}:
            # A failed create can leave a named archive with skipped files.
            # Rebuild from the retained immutable buffer; existing chunks remain
            # deduplicated until compaction after the successful replacement.
            self.command("delete", destination)
        self.command("create", "--compression=none", destination, ".", cwd=tree)
        # Do not remove the buffer merely because create returned: check the
        # saved archive including its payload chunks, also on crash/retry.
        self.command("extract", "--dry-run", destination)
        self.prune()

    def commit_sealed(self, capture):
        with locked(capture.root / "borg.lock"):
            garbage = capture.root / "committed-gc"
            garbage.mkdir(exist_ok=True)
            for batch in garbage.iterdir():
                shutil.rmtree(batch)
            for batch in sorted((capture.root / "batches").iterdir()):
                if not (batch / "sealed.json").exists():
                    continue
                identity = read_json(batch / "batch.json")["source"]
                if capture.active(identity) == batch:
                    continue
                name = "live-" + batch.name
                self.commit(name, batch)
                with locked(capture.root / "collector.lock"):
                    destination = garbage / batch.name
                    os.rename(batch, destination)
                    fsync_dir(batch.parent)
                    fsync_dir(garbage)
                shutil.rmtree(destination)
