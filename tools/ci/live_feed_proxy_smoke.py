#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Explicit real-nginx Controller.activate smoke tests, outside cheap discovery.

Run with unittest directly or pytest by this exact filename. Set
AEROBAG_TEST_NGINX for an extracted binary and AEROBAG_PROXY_SMOKE_ARTIFACT_DIR
for retained config, observed-state, HTTP and nginx logs. No host services run.
"""

from __future__ import annotations

import copy
import hashlib
import http.client
import json
import os
import pwd
import queue
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from datetime import datetime, timezone
from functools import partial
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from types import SimpleNamespace
from unittest import mock
from urllib.parse import urlsplit

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

import live_feed_compatibility as compatibility  # noqa: E402
import live_feed_launch  # noqa: E402
import reconcile_prod_releases as controller  # noqa: E402
import release_reconciler as releases  # noqa: E402


TIMEOUT = 8
OFFLINE_FIXTURE_PROGRAM = """#!/usr/bin/python3
import hashlib
import json
import sys
from pathlib import Path
assert len(sys.argv) == 3 and sys.argv[1] == '--describe-compatibility'
manifest = Path(sys.argv[2]).resolve(strict=True)
value = json.loads((Path(__file__).resolve().parents[1] / 'fixture-requirements.json').read_text())
value['startup_publication'] = {'path': str(manifest), 'sha256': hashlib.sha256(manifest.read_bytes()).hexdigest()}
print(json.dumps(value))
"""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class FixtureDaemon:
    """Independent direct-listener state with controlled streams and responses."""

    def __init__(self, tag: str, evidence: dict, log: Path):
        self.tag, self.evidence, self.log = tag, evidence, log
        self.streams: set[queue.Queue] = set()
        self.lock = threading.Lock()
        self.resource_started = threading.Event()
        self.release_resource = threading.Event()
        self.stopped = False
        daemon = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args):
                pass

            def json_response(self, status: int, value: dict):
                body = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                path = urlsplit(self.path).path
                with daemon.lock, daemon.log.open("a") as stream:
                    stream.write(json.dumps({"provider": daemon.tag, "path": self.path}) + "\n")
                if path == "/live-feeds/compatibility.json":
                    self.json_response(200, daemon.evidence)
                elif path in {"/live-feeds/status.json", "/live-feeds/v3/nested/identity.json"}:
                    self.json_response(200, {"provider": daemon.tag, "path": path})
                elif path == "/live-feeds/events":
                    messages = queue.Queue()
                    messages.put("initial")
                    with daemon.lock:
                        daemon.streams.add(messages)
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Cache-Control", "no-cache")
                    self.end_headers()
                    try:
                        while True:
                            event = messages.get(timeout=TIMEOUT * 2)
                            if event is None:
                                break
                            body = json.dumps({"provider": daemon.tag, "event": event})
                            self.wfile.write(f"event: catalog\ndata: {body}\n\n".encode())
                            self.wfile.flush()
                    except (BrokenPipeError, ConnectionResetError, queue.Empty):
                        pass
                    finally:
                        self.close_connection = True
                        with daemon.lock:
                            daemon.streams.discard(messages)
                elif path == f"/live-feeds/v3/held/{daemon.tag}.json":
                    daemon.resource_started.set()
                    if daemon.release_resource.wait(TIMEOUT):
                        self.json_response(200, {"provider": daemon.tag, "resource": "old-only"})
                    else:
                        self.json_response(504, {"error": "response barrier timed out"})
                else:
                    self.json_response(404, {"provider": daemon.tag, "error": "resource absent"})

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.02}, daemon=True)
        self.thread.start()
        self.endpoint = f"http://127.0.0.1:{self.server.server_port}"

    def emit(self, value: str | None):
        with self.lock:
            for stream in self.streams:
                stream.put(value)

    def close(self):
        if self.stopped:
            return
        self.release_resource.set()
        self.emit(None)
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=TIMEOUT)
        if self.thread.is_alive():
            raise RuntimeError(f"fixture daemon {self.tag} did not stop")
        self.stopped = True


class EventStream:
    def __init__(self, port: int, path: str):
        self.connection = http.client.HTTPConnection("127.0.0.1", port, timeout=TIMEOUT)
        self.connection.request("GET", path)
        self.response = self.connection.getresponse()
        if self.response.status != 200 or self.response.getheader("Content-Type") != "text/event-stream":
            self.close()
            raise AssertionError("SSE did not reach an upstream event stream")

    def event(self) -> dict:
        data = None
        for _ in range(16):
            line = self.response.readline()
            if not line:
                raise AssertionError("SSE closed before the expected event")
            if line.startswith(b"data: "):
                data = json.loads(line[6:])
            if line == b"\n" and data is not None:
                return data
        raise AssertionError("SSE did not provide a complete catalog event")

    def close(self):
        if hasattr(self, "response"):
            self.response.close()
        self.connection.close()


class LiveFeedProxySmokeTests(unittest.TestCase):
    def setUp(self):
        self.nginx = os.environ.get("AEROBAG_TEST_NGINX") or shutil.which("nginx")
        if not self.nginx or not os.access(self.nginx, os.X_OK):
            raise RuntimeError("nginx is required; install it or set AEROBAG_TEST_NGINX")
        destination = os.environ.get("AEROBAG_PROXY_SMOKE_ARTIFACT_DIR")
        if destination:
            Path(destination).mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix=f"{self._testMethodName}-", dir=destination))
        print(f"Live-feed proxy smoke evidence: {self.root}", flush=True)
        self.daemons: dict[str, FixtureDaemon] = {}
        self.streams: list[EventStream] = []
        self.process = None
        self.fail_nginx_validation = False
        self.stopped_units: list[str] = []
        self.addCleanup(self.close)
        self.instance = controller.Controller.__new__(controller.Controller)
        self.instance.artifact_root = self.root
        self.instance.args = SimpleNamespace(
            observed=self.root / "observed.json", force_production_tag=None,
            controller_preprocessor=self.root / "fixture-preprocessor", live_port_base=8100,
        )
        self.instance.desired = releases.DesiredReleases(
            releases.ReleaseBinding("prod"), releases.ReleaseBinding("stage"),
            (releases.SunsetBinding("old", "2099-01-01T00:00:00Z", releases.LiveFeedsPolicy.DEDICATED),),
        )
        self.instance.observed = releases.ObservedState(production="prod", staging="stage", sunset=["old"])
        self.env_root = self.root / "environment"
        self.env_root.mkdir()
        self.instance._live_feed_runtime = live_feed_launch.LiveFeedRuntime(
            self.root, self.instance.observed, port_base=8100,
            run=self.run_command, save=self.instance.save, env_root=self.env_root,
        )
        for tag in self.instance.desired.tags():
            self.prepare_release(tag)
        self.instance.retire_live_feed_instances = partial(
            self.instance.retire_live_feed_instances, env_root=self.env_root,
        )
        self.real_subprocess_run = subprocess.run
        # Packaging and Rust publication merging have independent suites. The
        # generation, binding checks, proxy config, nginx and public checks are real.
        for target, name, options in (
            (controller, "_run", {"side_effect": self.run_command}),
            (controller.live_feed_retirement.subprocess, "run", {"side_effect": self.retirement_command}),
            (controller.release_builder, "normalize_release_permissions", {}),
            (controller.release_builder, "validate_release_directory", {}),
        ):
            patcher = mock.patch.object(target, name, **options)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.bootstrap()
        self.instance.activate()
        self.wait_provider("/releases/old/live-feeds/v3/nested/identity.json", "old")

    def prepare_release(self, tag: str):
        release_root = self.root / "release-builds" / tag
        (release_root / "web").mkdir(parents=True)
        (release_root / "downloads").mkdir()
        (release_root / "web/index.html").write_text(f"<html>{tag}</html>")
        (release_root / "downloads/android-apk.json").write_text(json.dumps({"release": tag}))
        publication = self.root / "published" / tag
        (publication / "packaged").mkdir(parents=True)
        (publication / "unpacked").mkdir()
        manifest = publication / "product_artifacts.json"
        manifest.write_text(json.dumps({
            "schema_version": 1, "contracts": {"nav-db": "NAV25"}, "bundles": [],
            "artifact_roots": {"packaged": f"{tag}/packaged", "unpacked": f"{tag}/unpacked"},
        }))
        requirement = {
            "schema_version": 1, "wire_contracts": copy.deepcopy(compatibility.WIRE_INVENTORY),
            "notam_catalog": {
                "schema_version": compatibility.CLIENT_INVENTORY["nav_db"]["required_exact_keys"]["airport/notam-catalog"],
                "sha256": "a" * 64, "airport_count": 3,
            },
            "startup_publication": {"path": str(manifest), "sha256": sha256(manifest)},
        }
        (release_root / "fixture-requirements.json").write_text(json.dumps(requirement))
        inventory = release_root / "live-feed-compatibility.json"
        inventory.write_text(json.dumps(compatibility.WIRE_INVENTORY))
        (release_root / "bin").mkdir()
        binary = release_root / "bin/aerobag-live-feedsd"
        binary.write_text(OFFLINE_FIXTURE_PROGRAM)
        binary.chmod(0o755)
        (release_root / "release.json").write_text(json.dumps({
            "schema_version": 1, "tag": tag, "commit": "b" * 40,
            "artifacts": {
                "live_feeds_binary": {"filename": binary.name, "sha256": sha256(binary)},
                "live_feed_compatibility": {"filename": inventory.name, "sha256": sha256(inventory)},
            },
        }))
        record = releases.ObservedRelease(
            tag, "a" * 40, "b" * 40, build_status="passed", qualification_status="passed",
            deployment_status="passed", product_manifest=str(manifest), release_root=str(release_root),
            live_feed_requirements=requirement,
        )
        self.instance.observed.releases[tag] = record
        own = self.instance._live_feed_runtime.prepare(record, requirement)
        key, pinned = own.instance_id, Path(own.manifest)
        for root in own.roots:
            Path(root).mkdir(parents=True, exist_ok=True)
            (Path(root) / "retained-state").write_text(tag)
        (self.env_root / f"{key}.env").write_text(f"FIXTURE_RELEASE={tag}\n")
        evidence = {
            **copy.deepcopy(requirement), "executable_sha256": sha256(binary), "release_tag": tag,
            "launch_instance_id": key, "process_instance_id": f"{tag}-process-1",
            "configured_products": sorted(compatibility.WIRE_INVENTORY["products"]),
            "startup_publication": {"path": str(pinned), "sha256": sha256(pinned)},
            "projection_ready": True, "published_state_id": f"{tag}-state-1", "ready": True,
        }
        daemon = FixtureDaemon(tag, evidence, self.root / f"{tag}-http.jsonl")
        self.daemons[tag] = daemon
        own.endpoint, own.evidence, own.status = daemon.endpoint, copy.deepcopy(evidence), "running"
        record.live_feed_instance = record.live_feed_provider = key

    def own_instance(self, tag: str) -> releases.LiveFeedInstance:
        key = self.instance.observed.releases[tag].live_feed_instance
        return self.instance.observed.live_feed_instances[key]

    def retirement_command(self, command, **kwargs):
        if command[0] != "systemctl":
            return self.real_subprocess_run(command, **kwargs)
        with (self.root / "services.jsonl").open("a") as stream:
            stream.write(json.dumps(command) + "\n")
        for tag, daemon in self.daemons.items():
            own = self.own_instance(tag)
            if command == ["systemctl", "show", own.unit, "--property=ActiveState", "--value"]:
                return subprocess.CompletedProcess(command, 0, stdout="inactive\n" if daemon.stopped else "active\n")
            if command == ["systemctl", "disable", "--now", own.unit]:
                committed = releases.load_observed_state(self.instance.args.observed)
                serving = [committed.production, committed.staging, *committed.sunset]
                self.assertNotIn(own.instance_id, {
                    committed.releases[tag].live_feed_provider for tag in serving if tag is not None
                }, "a provider must be unbound in persisted state before service shutdown")
                self.assertEqual(committed.generation, self.instance.observed.generation)
                self.stopped_units.append(own.unit)
                daemon.close()
                return subprocess.CompletedProcess(command, 0)
        raise AssertionError(f"unexpected systemctl command; no host services are permitted: {command}")

    def bootstrap(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            self.port = listener.getsockname()[1]
        self.instance.args.public_origin = f"http://127.0.0.1:{self.port}"
        generation = self.root / "channel-generations/00000000"
        releases.materialize_channel_generation(
            generation, self.root / "published",
            production_manifests=[self.instance._manifest("prod"), self.instance._manifest("old")],
            staging_manifests=[self.instance._manifest("stage")],
            release_assets={tag: releases.ReleaseAssets(Path(record.release_root), self.daemons[tag].endpoint)
                            for tag, record in self.instance.observed.releases.items()},
        )
        (generation / "live-feeds.nginx.conf").write_text(releases.render_live_feed_nginx_routes(
            production_endpoint=self.daemons["prod"].endpoint,
            staging_endpoint=self.daemons["stage"].endpoint,
            release_endpoints={tag: daemon.endpoint for tag, daemon in self.daemons.items()},
        ))
        releases.activate_channel_generation(self.root, generation)
        self.config = self.root / "nginx.conf"
        user = f"user {pwd.getpwuid(os.getuid()).pw_name};" if os.getuid() == 0 else ""
        self.config.write_text(f"""{user}
worker_processes 1;
pid {self.root}/nginx.pid;
error_log {self.root}/nginx-error.log info;
worker_shutdown_timeout 30s;
events {{ worker_connections 128; }}
http {{
    access_log {self.root}/nginx-access.log;
    client_body_temp_path {self.root}/client-body;
    proxy_temp_path {self.root}/proxy-temp;
    fastcgi_temp_path {self.root}/fastcgi-temp;
    uwsgi_temp_path {self.root}/uwsgi-temp;
    scgi_temp_path {self.root}/scgi-temp;
    types {{ text/html html; application/json json; }}
    server {{
        listen 127.0.0.1:{self.port};
        add_header X-Smoke-Worker $pid always;
        include {self.root}/channel-current/live-feeds.nginx.conf;
        location / {{ root {self.root}/channel-current/production/web; index index.html; }}
        location /packages/ {{ alias {self.root}/channel-current/production/packages/; }}
        location /downloads/ {{ alias {self.root}/channel-current/production/downloads/; }}
    }}
}}
""")
        self.nginx_command("-t")
        self.process_log = (self.root / "nginx-process.log").open("w")
        self.process = subprocess.Popen(
            self.nginx_args("-g", "daemon off;"), stdout=self.process_log, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        self.wait_provider("/live-feeds/v3/nested/identity.json", "prod")

    def nginx_args(self, *arguments: str) -> list[str]:
        return [self.nginx, "-p", str(self.root), "-c", str(self.config),
                "-e", str(self.root / "nginx-error.log"), *arguments]

    def nginx_command(self, *arguments: str):
        with (self.root / "commands.log").open("a") as stream:
            command = self.nginx_args(*arguments)
            stream.write(json.dumps(command) + "\n")
            stream.flush()
            subprocess.run(command, check=True, timeout=TIMEOUT, stdout=stream, stderr=subprocess.STDOUT)

    def run_command(self, command, **_kwargs):
        if command == ["nginx", "-t"]:
            if self.fail_nginx_validation:
                self.fail_nginx_validation = False
                path = self.root / "channel-current/live-feeds.nginx.conf"
                with path.open("a") as stream:
                    stream.write("invalid_smoke_test_directive;\n")
            self.nginx_command("-t")
        elif command == ["systemctl", "reload", "nginx.service"]:
            connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=TIMEOUT)
            try:
                connection.request("GET", "/live-feeds/v3/nested/identity.json")
                response = connection.getresponse()
                old_worker = response.getheader("X-Smoke-Worker")
                response.read()
                self.assertTrue(old_worker and old_worker.isdecimal())
            finally:
                connection.close()
            self.nginx_command("-s", "reload")
            # Sending HUP is asynchronous. Observe the worker handoff before
            # testing new admission; old SSE connections must remain alive.
            deadline = time.monotonic() + TIMEOUT
            while time.monotonic() < deadline:
                self.assertIsNone(self.process.poll(), f"nginx exited; see {self.root}")
                lines = (self.root / "nginx-error.log").read_text().splitlines()
                if any(f"{old_worker}#" in line and "gracefully shutting down" in line for line in lines):
                    break
                threading.Event().wait(0.01)
            else:
                self.fail(f"nginx worker {old_worker} did not enter graceful shutdown; see {self.root}")
        elif command[:2] == [str(self.instance.args.controller_preprocessor), "merge-current-artifacts"]:
            # The real generation materializer already wrote this synthetic discovery.
            output = Path(command[command.index("--output") + 1])
            self.assertTrue(output.is_file())
        else:
            raise AssertionError(f"unexpected command, no host commands are permitted: {command}")

    def request(self, path: str) -> tuple[int, dict]:
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=TIMEOUT)
        try:
            connection.request("GET", path, headers={"Connection": "close"})
            response = connection.getresponse()
            self.assertIsNone(response.getheader("Location"), "routing must not use redirects")
            return response.status, json.loads(response.read())
        finally:
            connection.close()

    def wait_provider(self, path: str, tag: str):
        deadline = time.monotonic() + TIMEOUT
        last = None
        while time.monotonic() < deadline:
            if self.process is not None and self.process.poll() is not None:
                raise AssertionError(f"nginx exited with {self.process.returncode}; see {self.root}")
            try:
                last = self.request(path)
                if last[0] == 200 and last[1].get("provider") == tag:
                    return
            except (OSError, http.client.HTTPException) as error:
                last = repr(error)
            threading.Event().wait(0.01)
        self.fail(f"nginx route {path} did not become {tag}: {last}; logs: {self.root}")

    def stream(self) -> EventStream:
        stream = EventStream(self.port, "/releases/old/live-feeds/events")
        self.streams.append(stream)
        return stream

    def enable_sharing(self):
        self.instance.desired = replace(self.instance.desired, sunset=(
            releases.SunsetBinding("old", "2099-01-01T00:00:00Z"),
        ))

    def assert_isolated_routes(self, sunset_provider: str):
        for prefix, provider in (
            ("/live-feeds", "prod"), ("/releases/prod/live-feeds", "prod"),
            ("/staging/live-feeds", "stage"), ("/releases/stage/live-feeds", "stage"),
            ("/releases/old/live-feeds", sunset_provider),
        ):
            status, body = self.request(f"{prefix}/v3/nested/identity.json")
            self.assertEqual((status, body), (200, {"provider": provider, "path": "/live-feeds/v3/nested/identity.json"}))

    def close(self):
        for stream in self.streams:
            stream.close()
        for daemon in self.daemons.values():
            daemon.close()
        if self.process is not None:
            self.process.send_signal(signal.SIGQUIT)
            try:
                self.process.wait(timeout=TIMEOUT)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait(timeout=TIMEOUT)
                raise AssertionError(f"nginx did not stop; see {self.root}")
            finally:
                self.process_log.close()

    def test_shared_sunset_closes_old_stream_after_commit_and_retains_launch_inputs(self):
        self.assert_isolated_routes("old")
        stream = self.stream()
        self.assertEqual(stream.event(), {"provider": "old", "event": "initial"})
        old = self.own_instance("old")
        validate_public = self.instance.validate_public_production
        with ThreadPoolExecutor(max_workers=1) as pool:
            pending = pool.submit(self.request, "/releases/old/live-feeds/v3/held/old.json")

            def validate_during_switch():
                validate_public()
                self.assert_isolated_routes("prod")
                committed = releases.load_observed_state(self.instance.args.observed)
                self.assertEqual(committed.releases["old"].live_feed_provider, old.instance_id)
                self.assertEqual(self.stopped_units, [], "validation must precede retirement")
                self.daemons["old"].emit("after-reload-before-commit")
                self.assertEqual(stream.event(), {"provider": "old", "event": "after-reload-before-commit"})
                self.assertFalse(pending.done(), "outstanding response lost its explicit barrier")
                self.daemons["old"].release_resource.set()
                self.assertEqual(pending.result(timeout=TIMEOUT), (200, {"provider": "old", "resource": "old-only"}))

            try:
                self.assertTrue(self.daemons["old"].resource_started.wait(TIMEOUT), "old resource request did not start")
                self.enable_sharing()
                with mock.patch.object(self.instance, "validate_public_production", side_effect=validate_during_switch) as validation:
                    self.instance.activate()
                validation.assert_called_once_with()
            finally:
                self.daemons["old"].release_resource.set()
        self.assertEqual(self.stopped_units, [old.unit], "commit must promptly stop the unbound provider")
        self.assertTrue(self.daemons["old"].stopped)
        self.assertEqual(self.request("/releases/old/live-feeds/v3/held/old.json")[0], 404)
        self.assertEqual(stream.response.read(1), b"", "committed cutover must close old SSE without waiting for file grace")
        reconnected = self.stream()
        self.assertEqual(reconnected.event(), {"provider": "prod", "event": "initial"})
        self.daemons["prod"].emit("subsequent")
        self.assertEqual(reconnected.event(), {"provider": "prod", "event": "subsequent"})
        self.assert_isolated_routes("prod")
        observed = releases.load_observed_state(self.instance.args.observed)
        self.assertEqual(observed.releases["old"].live_feed_provider, self.own_instance("prod").instance_id)
        evidence = json.loads((self.root / "channel-current/live-feed-evidence.json").read_text())["releases"]["old"]
        self.assertEqual(evidence["requirements"], observed.releases["old"].live_feed_requirements)
        self.assertEqual(evidence["provider_instance"]["evidence"], self.own_instance("prod").evidence)
        self.assertEqual(evidence["provider_instance"]["instance_id"], self.own_instance("prod").instance_id)
        binding = json.loads((self.root / "channel-current/live-feed-bindings.json").read_text())["releases"]["old"]
        self.assertEqual(evidence["verification"], binding["verification"])
        self.assertEqual(observed.releases["stage"].live_feed_provider, self.own_instance("stage").instance_id)
        retired = observed.live_feed_instances[old.instance_id]
        self.assertEqual(retired.status, "stopped")
        self.assertIsNone(retired.evidence)
        self.assertGreater(datetime.fromisoformat(retired.draining_until_utc.replace("Z", "+00:00")), datetime.now(timezone.utc))
        self.assertEqual(sha256(Path(retired.manifest)), retired.manifest_sha256)
        for root in retired.roots:
            self.assertEqual((Path(root) / "retained-state").read_text(), "old")
        self.assertTrue((self.env_root / f"{old.instance_id}.env").is_file())
        self.assertTrue((Path(observed.releases["old"].release_root) / "web/index.html").is_file())
        pins = json.loads((self.root / releases.RELEASE_GC_ROOTS).read_text())["current_artifacts_paths"]
        self.assertTrue(set(retired.gc_paths).issubset(pins))
        self.instance.retire_live_feed_instances()
        self.assertEqual(self.stopped_units, [old.unit], "repeated retirement must not stop the provider again")
        self.assertEqual(retired.draining_until_utc, self.own_instance("old").draining_until_utc)

    def test_recovery_commits_switched_binding_before_disconnecting_old_stream(self):
        old = self.own_instance("old")
        before = releases.load_observed_state(self.instance.args.observed)
        stream = self.stream()
        self.assertEqual(stream.event(), {"provider": "old", "event": "initial"})
        self.enable_sharing()
        save = self.instance.save

        def fail_commit():
            if self.instance.observed.generation == before.generation + 1:
                raise RuntimeError("simulated crash before activation commit")
            save()

        with mock.patch.object(self.instance, "save", side_effect=fail_commit):
            with self.assertRaisesRegex(RuntimeError, "simulated crash before activation commit"):
                self.instance.activate()
        self.assert_isolated_routes("prod")
        self.assertEqual(self.stopped_units, [])
        self.daemons["old"].emit("before-recovery")
        self.assertEqual(stream.event(), {"provider": "old", "event": "before-recovery"})
        self.instance.observed = releases.load_observed_state(self.instance.args.observed)
        self.instance._live_feed_runtime.observed = self.instance.observed
        self.assertEqual(self.instance.observed.generation, before.generation)
        self.assertEqual(self.instance.observed.releases["old"].live_feed_provider, old.instance_id)
        self.assertTrue(self.instance.recover_activated_generation())
        self.assertEqual(self.stopped_units, [old.unit], "recovery must also disconnect unbound providers promptly")
        self.assertEqual(stream.response.read(1), b"")
        reconnected = self.stream()
        self.assertEqual(reconnected.event(), {"provider": "prod", "event": "initial"})
        self.assert_isolated_routes("prod")
        committed = releases.load_observed_state(self.instance.args.observed)
        self.assertEqual(committed.generation, before.generation + 1)
        self.assertEqual(committed.releases["old"].live_feed_provider, self.own_instance("prod").instance_id)
        self.assertEqual(committed.live_feed_instances[old.instance_id].status, "stopped")
        self.assertTrue(Path(old.manifest).is_file())
        self.assertFalse(self.instance.recover_activated_generation())
        self.assertEqual(self.stopped_units, [old.unit])

    def test_incompatible_catalog_keeps_sunset_dedicated_with_staging_unchanged(self):
        self.enable_sharing()
        self.instance.observed.releases["old"].live_feed_requirements["notam_catalog"]["sha256"] = "f" * 64
        self.instance.activate()
        self.assert_isolated_routes("old")
        self.assertEqual(self.instance.observed.releases["old"].live_feed_reason, "dedicated: NOTAM catalog differs")
        self.assertEqual(self.stopped_units, [])

    def test_nginx_validation_failure_rolls_back_without_changing_active_binding(self):
        before = (self.root / "channel-current").resolve()
        stream = self.stream()
        self.assertEqual(stream.event()["provider"], "old")
        self.enable_sharing()
        self.fail_nginx_validation = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.instance.activate()
        self.assertEqual((self.root / "channel-current").resolve(), before)
        self.assertEqual(releases.load_observed_state(self.instance.args.observed).releases["old"].live_feed_provider,
                         self.own_instance("old").instance_id)
        self.assertEqual(self.stopped_units, [])
        self.assert_isolated_routes("old")
        self.daemons["old"].emit("after-rollback")
        self.assertEqual(stream.event(), {"provider": "old", "event": "after-rollback"})

    def test_direct_process_change_before_activation_rejects_stale_sharing_approval(self):
        before = (self.root / "channel-current").resolve()
        self.enable_sharing()
        self.daemons["prod"].evidence["process_instance_id"] = "prod-process-restarted"
        with self.assertRaisesRegex(releases.ReleaseConfigError, "evidence changed"):
            self.instance.activate()
        self.assertEqual((self.root / "channel-current").resolve(), before)
        self.assert_isolated_routes("old")
        self.assertEqual(self.stopped_units, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
