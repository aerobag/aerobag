#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import argparse
import ctypes
import http.client
import json
import mimetypes
import os
import signal
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

from admin_index import admin_index_html


REPO_ROOT = Path(__file__).resolve().parents[1]
PREPROCESSOR_MANIFEST = REPO_ROOT / "product" / "preprocessor" / "Cargo.toml"
WATCH_BUILD_LOG = REPO_ROOT / "product" / "preprocessor" / "scripts" / "watch_build_log.py"
PIPELINE_HEALTH = REPO_ROOT / "product" / "preprocessor" / "scripts" / "pipeline_health.py"
FAA_CYCLE_CALENDAR = REPO_ROOT / "deploy" / "faa-cycle-calendar.json"
DEFAULT_FRONT_DOOR = "0.0.0.0:18080"
DEFAULT_LIVE_FEEDS = "127.0.0.1:18095"
DEFAULT_CLOUD_SERVER = "127.0.0.1:18096"
DEFAULT_BUILD_WATCH = "127.0.0.1:18097"
DEFAULT_PIPELINE_HEALTH = "127.0.0.1:18098"
LIVE_FEEDS_CONTRACT_PATH = "v2"
DEFAULT_NMS_NOTAMS_CONFIG = Path(
    "/root/aerobag-credentials/dev-stack/nms-notams-staging.json"
)
DEFAULT_CLOUD_SERVER_SECRET = Path(
    "/root/aerobag-credentials/dev-stack/aerobag-cloud-server.bin"
)
DEFAULT_CLOUD_SERVER_POLICY = REPO_ROOT / "deploy" / "aerobag-cloud-policy.json"


def utc_now_text() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def artifact_root_default() -> Path:
    env_value = os.environ.get("AEROBAG_ARTIFACT_WRITE_PATH")
    if env_value:
        path = Path(env_value).expanduser()
        return path if path.is_absolute() else (REPO_ROOT / path).resolve()
    pointer = REPO_ROOT / ".aerobag-artifact-write-path"
    if pointer.is_file():
        path = Path(pointer.read_text(encoding="utf-8").strip()).expanduser()
        return path if path.is_absolute() else (REPO_ROOT / path).resolve()
    return REPO_ROOT.parent / "aerobag-artifacts"


def parse_listen(value: str) -> tuple[str, int]:
    if ":" not in value:
        return value or "127.0.0.1", 80
    host, port = value.rsplit(":", 1)
    return host or "127.0.0.1", int(port)


def display_url(listen: str) -> str:
    host, port = parse_listen(listen)
    if host in {"0.0.0.0", "::"}:
        host = "aerobag-dev.iac.jonh.net"
    return f"http://{host}:{port}"


def read_json_file(path: Path) -> object | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return None


def file_age_seconds(path: Path) -> int | None:
    try:
        return max(0, int(time.time() - path.stat().st_mtime))
    except FileNotFoundError:
        return None


def current_artifacts_summary(path: Path) -> dict[str, object]:
    payload = read_json_file(path)
    return {
        "path": str(path),
        "exists": path.is_file(),
        "age_seconds": file_age_seconds(path),
        "manifest_count": len(payload) if isinstance(payload, list) else None,
        "contracts": [
            manifest.get("contracts", {})
            for manifest in payload
            if isinstance(manifest, dict)
        ]
        if isinstance(payload, list)
        else [],
    }


def safe_static_path(root: Path, relative: str) -> Path | None:
    root = root.resolve()
    candidate = (root / relative.lstrip("/")).resolve()
    try:
        candidate.relative_to(root)
    except ValueError:
        return None
    return candidate


def content_type(path: Path) -> str:
    guessed, _encoding = mimetypes.guess_type(path.name)
    if guessed:
        return guessed
    if path.suffix == ".wasm":
        return "application/wasm"
    return "application/octet-stream"


def child_preexec() -> None:
    os.setsid()
    try:
        libc = ctypes.CDLL("libc.so.6")
        libc.prctl(1, signal.SIGTERM)
    except Exception:
        pass
    if os.getppid() == 1:
        os.kill(os.getpid(), signal.SIGTERM)


@dataclass
class DevStackConfig:
    artifact_root: Path
    stack_root: Path
    source_commit: str
    web_dist: Path | None
    target_dir: Path
    listen: str
    live_feeds_listen: str
    cloud_server_listen: str
    build_watch_listen: str
    pipeline_health_listen: str
    live_feed_fetch_mode: str
    nms_notams_enabled: bool
    nms_notams_config: Path
    nms_notams_state_root: Path
    cloud_server_secret: Path
    cloud_server_policy: Path
    cloud_tiny_creation_buckets: str | None
    skip_binary_build: bool
    disable_live_feeds: bool
    disable_cloud_server: bool
    disable_build_watch: bool
    disable_pipeline_health: bool

    @property
    def published_root(self) -> Path:
        return self.artifact_root / "published"

    @property
    def live_root(self) -> Path:
        return self.stack_root / "live-feeds"

    @property
    def live_contract_root(self) -> Path:
        return self.live_root / LIVE_FEEDS_CONTRACT_PATH

    @property
    def scratch_root(self) -> Path:
        return self.artifact_root / "scratch" / "dev-stack" / "live-feeds"

    @property
    def fetch_cache_root(self) -> Path:
        return self.artifact_root / "cache" / "fetch"

    @property
    def health_root(self) -> Path:
        return self.stack_root / "health"

    @property
    def cloud_data_root(self) -> Path:
        return self.stack_root / "state" / "aerobag-cloud-server"

    @property
    def cloud_runtime_policy(self) -> Path:
        return self.stack_root / "state" / "aerobag-cloud-policy.json"

    @property
    def deploy_health_path(self) -> Path:
        return self.health_root / "status.json"

    @property
    def build_log_path(self) -> Path:
        return (
            self.artifact_root
            / "logs"
            / "orchestrator"
            / "published"
            / "master.log"
        )

    @property
    def live_feeds_binary(self) -> Path:
        return self.target_dir / "debug" / "aerobag-live-feedsd"

    @property
    def cloud_server_binary(self) -> Path:
        return self.target_dir / "debug" / "aerobag-cloud-serverd"


@dataclass
class ManagedProcess:
    name: str
    process: subprocess.Popen[str]

    def poll(self) -> int | None:
        return self.process.poll()

    def terminate_group(self) -> None:
        if self.process.poll() is not None:
            return
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
        except ProcessLookupError:
            return

    def kill_group(self) -> None:
        if self.process.poll() is not None:
            return
        try:
            os.killpg(self.process.pid, signal.SIGKILL)
        except ProcessLookupError:
            return


class DevStack:
    def __init__(self, config: DevStackConfig) -> None:
        self.config = config
        self.children: list[ManagedProcess] = []
        self.stop = threading.Event()

    def start(self) -> None:
        self.prepare_dirs()
        if not self.config.skip_binary_build and (
            not self.config.disable_live_feeds or not self.config.disable_cloud_server
        ):
            self.build_binaries()
        if not self.config.disable_live_feeds:
            self.start_child("live-feeds", self.live_feeds_command())
        if not self.config.disable_cloud_server:
            if not self.config.cloud_server_secret.is_file():
                raise RuntimeError(
                    f"ACS server secret is missing: {self.config.cloud_server_secret}"
                )
            self.write_cloud_policy()
            self.start_child("aerobag-cloud-server", self.cloud_server_command())
        if not self.config.disable_build_watch:
            self.start_child("build-watch", self.build_watch_command())
        if not self.config.disable_pipeline_health:
            self.start_child("pipeline-health", self.pipeline_health_command())

    def prepare_dirs(self) -> None:
        for path in [
            self.config.stack_root,
            self.config.live_root,
            self.config.live_contract_root,
            self.config.scratch_root,
            self.config.fetch_cache_root,
            self.config.health_root,
            self.config.target_dir,
            self.config.nms_notams_state_root.parent,
            self.config.cloud_data_root,
        ]:
            path.mkdir(parents=True, exist_ok=True)
        self.write_health()

    def write_cloud_policy(self) -> None:
        policy = json.loads(self.config.cloud_server_policy.read_text(encoding="utf-8"))
        if self.config.cloud_tiny_creation_buckets:
            limits = policy["rate_limits"]
            network_capacity, global_capacity = (
                (1, 100)
                if self.config.cloud_tiny_creation_buckets == "network"
                else (100, 1)
            )
            limits["account_creation_per_network"].update(
                capacity=network_capacity,
                refill_amount=1,
            )
            limits["account_creation_global"].update(
                capacity=global_capacity,
                refill_amount=1,
            )
        self.config.cloud_runtime_policy.write_text(
            json.dumps(policy, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    def build_binaries(self) -> None:
        env = self.child_env()
        commands: list[list[str]] = []
        if not self.config.disable_live_feeds:
            commands.append([
                "cargo", "build", "--manifest-path", str(PREPROCESSOR_MANIFEST),
                "-p", "live-feeds-daemon",
            ])
        if not self.config.disable_cloud_server:
            commands.append([
                "cargo", "build", "--manifest-path", str(REPO_ROOT / "services" / "Cargo.toml"),
                "-p", "aerobag-cloud-server",
            ])
        for command in commands:
            print(f"+ {' '.join(command)}", flush=True)
            subprocess.run(command, cwd=REPO_ROOT, env=env, check=True)

    def start_child(self, name: str, command: list[str]) -> None:
        print(f"+ start {name}: {' '.join(command)}", flush=True)
        process = subprocess.Popen(
            command,
            cwd=REPO_ROOT,
            env=self.child_env(),
            text=True,
            preexec_fn=child_preexec,
        )
        self.children.append(ManagedProcess(name=name, process=process))

    def child_env(self) -> dict[str, str]:
        env = os.environ.copy()
        env.update(
            {
                "CARGO_TARGET_DIR": str(self.config.target_dir),
                "AEROBAG_ARTIFACT_WRITE_PATH": str(self.config.artifact_root),
                "AEROBAG_ARTIFACT_READ_PATH": str(self.config.published_root),
                "ARTIFACT_ROOT": str(self.config.artifact_root),
                "DATA_ROOT": str(self.config.stack_root),
                "AEROBAG_LIVE_FEEDS_LISTEN": self.config.live_feeds_listen,
                "AEROBAG_BUILD_WATCH_LISTEN": self.config.build_watch_listen,
                "AEROBAG_PIPELINE_HEALTH_LISTEN": self.config.pipeline_health_listen,
                "PATH": (
                    "/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:"
                    "/usr/sbin:/usr/bin:/sbin:/bin"
                ),
            }
        )
        return env

    def live_feeds_command(self) -> list[str]:
        command = [
            str(self.config.live_feeds_binary),
            "--live-root",
            str(self.config.live_root),
            "--scratch-root",
            str(self.config.scratch_root),
            "--fetch-cache-root",
            str(self.config.fetch_cache_root),
            "--fetch-cache-mode",
            self.config.live_feed_fetch_mode,
            "--listen",
            self.config.live_feeds_listen,
        ]
        if self.config.nms_notams_enabled:
            command.extend(
                [
                    "--nms-notams-config",
                    str(self.config.nms_notams_config),
                    "--nms-notams-state-root",
                    str(self.config.nms_notams_state_root),
                ]
            )
        return command

    def cloud_server_command(self) -> list[str]:
        command = [
            str(self.config.cloud_server_binary),
            "serve",
            "--data-root",
            str(self.config.cloud_data_root),
            "--server-secret",
            str(self.config.cloud_server_secret),
            "--policy",
            str(self.config.cloud_runtime_policy),
            "--listen",
            self.config.cloud_server_listen,
        ]
        return command

    def build_watch_command(self) -> list[str]:
        return [
            str(WATCH_BUILD_LOG),
            str(self.config.build_log_path),
            "--serve",
            self.config.build_watch_listen,
            "--refresh-seconds",
            "2",
        ]

    def pipeline_health_command(self) -> list[str]:
        return [
            str(PIPELINE_HEALTH),
            "--artifact-root",
            str(self.config.artifact_root),
            "--data-root",
            str(self.config.stack_root),
            "--health-root",
            str(self.config.health_root),
            "--current-artifacts",
            str(self.config.published_root / "current_artifacts.json"),
            "--deploy-health",
            str(self.config.deploy_health_path),
            "--live-feeds-status-url",
            f"http://{self.config.live_feeds_listen}/live-feeds/status.json",
            "--cloud-status-url",
            f"http://{self.config.cloud_server_listen}/cloud/v1/status",
            "--cloud-status-secret",
            str(self.config.cloud_server_secret),
            "--build-watch-url",
            f"http://{self.config.build_watch_listen}/api/state",
            "--calendar",
            str(FAA_CYCLE_CALENDAR),
            "--listen",
            self.config.pipeline_health_listen,
            "--poll-seconds",
            "30",
        ]

    def write_health(self) -> None:
        self.config.health_root.mkdir(parents=True, exist_ok=True)
        payload = {
            "schema_version": 1,
            "generated_at_utc": utc_now_text(),
            "mode": "dev-stack",
            "artifact_root": str(self.config.artifact_root),
            "published_root": str(self.config.published_root),
            "live_contract_root": str(self.config.live_contract_root),
            "front_door": display_url(self.config.listen),
            "services": {
                child.name: {
                    "pid": child.process.pid,
                    "alive": child.process.poll() is None,
                    "returncode": child.process.poll(),
                }
                for child in self.children
            },
            "current_artifacts": current_artifacts_summary(
                self.config.published_root / "current_artifacts.json"
            ),
            "routes": {
                "packages": "/packages/",
                "live_feeds": "/live-feeds/",
                "cloud": "/cloud/",
                "build_watch": "/build-watch/",
                "pipeline_health": "/pipeline-health/",
            },
        }
        tmp = self.config.deploy_health_path.with_suffix(".json.tmp")
        tmp.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        tmp.replace(self.config.deploy_health_path)

    def health_writer(self) -> None:
        while not self.stop.wait(5):
            self.write_health()

    def check_children(self) -> None:
        for child in self.children:
            returncode = child.poll()
            if returncode is not None:
                print(
                    f"{child.name} exited with status {returncode}; stopping dev stack",
                    file=sys.stderr,
                    flush=True,
                )
                self.stop.set()
                return

    def shutdown(self) -> None:
        if not self.children:
            return
        print("stopping dev stack children...", flush=True)
        for child in self.children:
            child.terminate_group()
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if all(child.poll() is not None for child in self.children):
                break
            time.sleep(0.1)
        for child in self.children:
            child.kill_group()
        self.write_health()


class DevStackServer(ThreadingHTTPServer):
    allow_reuse_address = True
    daemon_threads = True


def make_handler(stack: DevStack):
    config = stack.config

    class Handler(BaseHTTPRequestHandler):
        server_version = "AerobagDevStack/1"

        def log_message(self, format: str, *args: object) -> None:
            return

        def do_HEAD(self) -> None:
            self.handle_request(send_body=False)

        def do_GET(self) -> None:
            self.handle_request(send_body=True)

        def do_POST(self) -> None:
            self.handle_request(send_body=True)

        def do_PUT(self) -> None:
            self.handle_request(send_body=True)

        def do_DELETE(self) -> None:
            self.handle_request(send_body=True)

        def do_OPTIONS(self) -> None:
            parsed = urlparse(self.path)
            path = parsed.path
            if (path == "/live-feeds" or path.startswith("/live-feeds/")
                    or path == "/cloud" or path.startswith("/cloud/")):
                self.send_cors_options()
                return
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()

        def handle_request(self, send_body: bool) -> None:
            parsed = urlparse(self.path)
            path = parsed.path
            if path == "/health.json":
                self.serve_file(config.deploy_health_path, send_body, no_store=True)
            elif path == "/packages" or path.startswith("/packages/"):
                relative = path.removeprefix("/packages").lstrip("/") or "current_artifacts.json"
                self.serve_static(config.published_root, relative, send_body)
            elif path == "/live-feeds" or path.startswith("/live-feeds/"):
                if config.disable_live_feeds:
                    self.send_text(503, "live-feeds disabled\n", send_body)
                else:
                    self.proxy(config.live_feeds_listen, path, parsed.query, send_body, cors=True)
            elif path == "/cloud/v1/status":
                self.send_text(404, "not found\n", send_body)
            elif path == "/cloud" or path.startswith("/cloud/"):
                if config.disable_cloud_server:
                    self.send_text(503, "Aerobag Cloud disabled\n", send_body)
                else:
                    self.proxy(config.cloud_server_listen, path, parsed.query, send_body)
            elif path == "/build-watch" or path.startswith("/build-watch/"):
                if config.disable_build_watch:
                    self.send_text(503, "build-watch disabled\n", send_body)
                else:
                    upstream_path = path.removeprefix("/build-watch") or "/"
                    self.proxy(config.build_watch_listen, upstream_path, parsed.query, send_body)
            elif path == "/pipeline-health" or path.startswith("/pipeline-health/"):
                if config.disable_pipeline_health:
                    self.send_text(503, "pipeline-health disabled\n", send_body)
                else:
                    self.proxy(config.pipeline_health_listen, path, parsed.query, send_body)
            elif path in {"/admin", "/admin/"}:
                self.send_html(200, index_html(config), send_body)
            else:
                self.serve_web_or_index(path, send_body)

        def serve_web_or_index(self, path: str, send_body: bool) -> None:
            if config.web_dist is not None and config.web_dist.is_dir():
                relative = path.lstrip("/") or "index.html"
                candidate = safe_static_path(config.web_dist, relative)
                if candidate is not None and candidate.is_file():
                    self.serve_file(candidate, send_body, no_store=False)
                    return
                index = config.web_dist / "index.html"
                if index.is_file():
                    self.serve_file(index, send_body, no_store=False)
                    return
            self.send_html(200, index_html(config), send_body)

        def serve_static(self, root: Path, relative: str, send_body: bool) -> None:
            path = safe_static_path(root, relative)
            if path is None or not path.is_file():
                self.send_text(404, "not found\n", send_body)
                return
            self.serve_file(path, send_body, no_store=False)

        def serve_file(self, path: Path, send_body: bool, *, no_store: bool) -> None:
            try:
                stat = path.stat()
                content_type_value = content_type(path)
                self.send_response(200)
                self.send_header("Content-Type", content_type_value)
                self.send_header("Content-Length", str(stat.st_size))
                self.send_header(
                    "Cache-Control",
                    "no-store" if no_store else "public, max-age=300",
                )
                self.end_headers()
                if send_body:
                    with path.open("rb") as stream:
                        while chunk := stream.read(1024 * 256):
                            self.wfile.write(chunk)
            except FileNotFoundError:
                self.send_text(404, "not found\n", send_body)

        def proxy(
            self,
            listen: str,
            path: str,
            query: str,
            send_body: bool,
            *,
            cors: bool = False,
        ) -> None:
            host, port = parse_listen(listen)
            target = path
            if query:
                target = f"{target}?{query}"
            connection = http.client.HTTPConnection(host, port, timeout=3600)
            response_started = False
            try:
                request_body = None
                content_length = self.headers.get("Content-Length")
                if content_length:
                    length = int(content_length)
                    if length > 2 * 1024 * 1024:
                        self.send_text(413, "request too large\n", send_body)
                        return
                    request_body = self.rfile.read(length)
                forwarded_headers = {
                    header: value
                    for header, value in self.headers.items()
                    if header.lower().startswith("aerobag-")
                    or header.lower() in {"content-type", "last-event-id"}
                }
                forwarded_headers["Host"] = listen
                forwarded_headers["Aerobag-Client-Address"] = self.client_address[0]
                connection.request(
                    "HEAD" if not send_body else self.command,
                    target,
                    body=request_body,
                    headers=forwarded_headers,
                )
                response = connection.getresponse()
                self.send_response(response.status, response.reason)
                has_cors_origin = False
                for header, value in response.getheaders():
                    lower = header.lower()
                    if lower in {
                        "connection",
                        "keep-alive",
                        "proxy-authenticate",
                        "proxy-authorization",
                        "te",
                        "trailers",
                        "transfer-encoding",
                        "upgrade",
                    }:
                        continue
                    if lower == "access-control-allow-origin":
                        has_cors_origin = True
                    self.send_header(header, value)
                self.send_header("Cache-Control", "no-store")
                if cors and not has_cors_origin:
                    self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                response_started = True
                if send_body:
                    chunk_size = 1 if response.getheader("Content-Type", "").startswith("text/event-stream") else 1024 * 64
                    while chunk := response.read(chunk_size):
                        self.wfile.write(chunk)
                        self.wfile.flush()
            except (ConnectionError, OSError, TimeoutError) as exc:
                if not response_started:
                    self.send_text(502, f"upstream {listen} unavailable: {exc}\n", send_body)
            finally:
                connection.close()

        def send_cors_options(self) -> None:
            self.send_response(204)
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, HEAD, OPTIONS")
            self.send_header("Access-Control-Allow-Headers", "Last-Event-ID, Cache-Control, Content-Type, Aerobag-Contract, Aerobag-Account, Aerobag-Key-Id, Aerobag-Signature-Algorithm, Aerobag-Timestamp-Ms, Aerobag-Nonce, Aerobag-Body-SHA256, Aerobag-Signature")
            self.send_header("Access-Control-Max-Age", "600")
            self.send_header("Content-Length", "0")
            self.end_headers()

        def send_text(self, status: int, body: str, send_body: bool) -> None:
            payload = body.encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            if send_body:
                self.wfile.write(payload)

        def send_html(self, status: int, body: str, send_body: bool) -> None:
            payload = body.encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            if send_body:
                self.wfile.write(payload)

    return Handler


def index_html(config: DevStackConfig) -> str:
    return admin_index_html(
        title="Aerobag Dev Stack",
        front_door=display_url(config.listen),
        commit_hash=config.source_commit,
        cycle_products_root=str(config.published_root),
        live_feed_output_root=str(config.live_contract_root),
    )


def parse_args() -> argparse.Namespace:
    artifact_root = artifact_root_default()
    parser = argparse.ArgumentParser(
        description="Run a local dev analog of the Aerobag production server stack."
    )
    parser.add_argument("--artifact-root", type=Path, default=artifact_root)
    parser.add_argument(
        "--stack-root",
        type=Path,
        default=None,
        help="dev stack mutable root (default: artifact-root/dev-stack)",
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=None,
        help="Cargo target dir (default: artifact-root/target)",
    )
    parser.add_argument("--listen", default=DEFAULT_FRONT_DOOR)
    parser.add_argument("--live-feeds-listen", default=DEFAULT_LIVE_FEEDS)
    parser.add_argument("--cloud-server-listen", default=DEFAULT_CLOUD_SERVER)
    parser.add_argument("--build-watch-listen", default=DEFAULT_BUILD_WATCH)
    parser.add_argument("--pipeline-health-listen", default=DEFAULT_PIPELINE_HEALTH)
    parser.add_argument("--live-feed-fetch-mode", default="fill", choices=["fill", "offline"])
    parser.add_argument(
        "--nms-notams-config",
        type=Path,
        default=DEFAULT_NMS_NOTAMS_CONFIG,
        help=(
            "operator-owned NMS API credential file; dev-stack enables NOTAM ingestion "
            "when this exists unless --disable-nms-notams is set"
        ),
    )
    parser.add_argument(
        "--cloud-server-secret",
        type=Path,
        default=DEFAULT_CLOUD_SERVER_SECRET,
        help="operator-owned 32-byte ACS daemon secret",
    )
    parser.add_argument(
        "--cloud-server-policy",
        type=Path,
        default=DEFAULT_CLOUD_SERVER_POLICY,
        help="versioned ACS runtime policy JSON",
    )
    parser.add_argument(
        "--nms-notams-state-root",
        type=Path,
        default=None,
        help="durable NMS NOTAM state root (default: stack-root/state/nms-notams)",
    )
    parser.add_argument(
        "--web-dist",
        type=Path,
        default=None,
        help="optional built web app dist to serve at /",
    )
    parser.add_argument("--skip-binary-build", action="store_true")
    parser.add_argument("--disable-live-feeds", action="store_true")
    parser.add_argument("--disable-cloud-server", action="store_true")
    parser.add_argument(
        "--cloud-tiny-creation-buckets",
        choices=["network", "global"],
        help="use a tiny network or global account-creation bucket for UX testing",
    )
    parser.add_argument("--disable-nms-notams", action="store_true")
    parser.add_argument("--disable-build-watch", action="store_true")
    parser.add_argument("--disable-pipeline-health", action="store_true")
    parser.add_argument("--check-config", action="store_true")
    return parser.parse_args()


def config_from_args(args: argparse.Namespace) -> DevStackConfig:
    artifact_root = args.artifact_root.resolve()
    stack_root = (args.stack_root or artifact_root / "dev-stack").resolve()
    target_dir = (args.target_dir or artifact_root / "target").resolve()
    web_dist = args.web_dist.resolve() if args.web_dist else None
    nms_notams_config = args.nms_notams_config.resolve()
    nms_notams_enabled = (
        not args.disable_live_feeds
        and not args.disable_nms_notams
        and nms_notams_config.is_file()
    )
    return DevStackConfig(
        artifact_root=artifact_root,
        stack_root=stack_root,
        source_commit=subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=REPO_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            check=True,
        ).stdout.strip(),
        web_dist=web_dist,
        target_dir=target_dir,
        listen=args.listen,
        live_feeds_listen=args.live_feeds_listen,
        cloud_server_listen=args.cloud_server_listen,
        build_watch_listen=args.build_watch_listen,
        pipeline_health_listen=args.pipeline_health_listen,
        live_feed_fetch_mode=args.live_feed_fetch_mode,
        nms_notams_enabled=nms_notams_enabled,
        nms_notams_config=nms_notams_config,
        nms_notams_state_root=(
            args.nms_notams_state_root or stack_root / "state" / "nms-notams"
        ).resolve(),
        cloud_server_secret=args.cloud_server_secret.resolve(),
        cloud_server_policy=args.cloud_server_policy.resolve(),
        cloud_tiny_creation_buckets=args.cloud_tiny_creation_buckets,
        skip_binary_build=args.skip_binary_build,
        disable_live_feeds=args.disable_live_feeds,
        disable_cloud_server=args.disable_cloud_server,
        disable_build_watch=args.disable_build_watch,
        disable_pipeline_health=args.disable_pipeline_health,
    )


def print_config(config: DevStackConfig) -> None:
    print(
        json.dumps(
            {
                "front_door": display_url(config.listen),
                "artifact_root": str(config.artifact_root),
                "published_root": str(config.published_root),
                "stack_root": str(config.stack_root),
                "live_root": str(config.live_root),
                "live_contract_root": str(config.live_contract_root),
                "target_dir": str(config.target_dir),
                "live_feeds_listen": config.live_feeds_listen,
                "cloud_server_listen": config.cloud_server_listen,
                "cloud_data_root": str(config.cloud_data_root),
                "cloud_server_secret": str(config.cloud_server_secret),
                "cloud_server_policy": str(config.cloud_server_policy),
                "cloud_runtime_policy": str(config.cloud_runtime_policy),
                "cloud_tiny_creation_buckets": config.cloud_tiny_creation_buckets,
                "nms_notams_enabled": config.nms_notams_enabled,
                "nms_notams_config": str(config.nms_notams_config),
                "nms_notams_state_root": str(config.nms_notams_state_root),
                "build_watch_listen": config.build_watch_listen,
                "pipeline_health_listen": config.pipeline_health_listen,
                "web_dist": str(config.web_dist) if config.web_dist else None,
            },
            indent=2,
            sort_keys=True,
        )
    )


def main() -> int:
    args = parse_args()
    config = config_from_args(args)
    if args.check_config:
        print_config(config)
        return 0

    stack = DevStack(config)

    def handle_signal(signum: int, _frame: object) -> None:
        print(f"received signal {signum}; stopping dev stack", flush=True)
        stack.stop.set()

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    stack.start()
    stack.write_health()
    threading.Thread(target=stack.health_writer, daemon=True).start()

    host, port = parse_listen(config.listen)
    server = DevStackServer((host, port), make_handler(stack))
    server.timeout = 1
    print_config(config)
    print(f"serving dev stack on {display_url(config.listen)}", flush=True)

    try:
        while not stack.stop.is_set():
            server.handle_request()
            stack.check_children()
    finally:
        stack.stop.set()
        stack.shutdown()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
