#!/usr/bin/env python3
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


REPO_ROOT = Path(__file__).resolve().parents[1]
PREPROCESSOR_MANIFEST = REPO_ROOT / "product" / "preprocessor" / "Cargo.toml"
WATCH_BUILD_LOG = REPO_ROOT / "product" / "preprocessor" / "scripts" / "watch_build_log.py"
PIPELINE_HEALTH = REPO_ROOT / "product" / "preprocessor" / "scripts" / "pipeline_health.py"
FAA_CYCLE_CALENDAR = REPO_ROOT / "deploy" / "faa-cycle-calendar.json"
DEFAULT_FRONT_DOOR = "0.0.0.0:18080"
DEFAULT_LIVE_FEEDS = "127.0.0.1:18095"
DEFAULT_BUILD_WATCH = "127.0.0.1:18097"
DEFAULT_PIPELINE_HEALTH = "127.0.0.1:18098"


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
    web_dist: Path | None
    target_dir: Path
    listen: str
    live_feeds_listen: str
    build_watch_listen: str
    pipeline_health_listen: str
    live_feed_fetch_mode: str
    skip_binary_build: bool
    disable_live_feeds: bool
    disable_build_watch: bool
    disable_pipeline_health: bool

    @property
    def published_root(self) -> Path:
        return self.artifact_root / "published"

    @property
    def live_root(self) -> Path:
        return self.stack_root / "live-feeds"

    @property
    def scratch_root(self) -> Path:
        return self.artifact_root / "private-work" / "dev-stack" / "live-feeds"

    @property
    def fetch_cache_root(self) -> Path:
        return self.artifact_root / "cache" / "fetch"

    @property
    def health_root(self) -> Path:
        return self.stack_root / "health"

    @property
    def deploy_health_path(self) -> Path:
        return self.health_root / "status.json"

    @property
    def build_log_path(self) -> Path:
        return (
            self.artifact_root
            / "private-work"
            / "orchestrator-logs"
            / "published"
            / "master.log"
        )

    @property
    def live_feeds_binary(self) -> Path:
        return self.target_dir / "debug" / "aerobag-live-feedsd"


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
        if not self.config.skip_binary_build and not self.config.disable_live_feeds:
            self.build_binaries()
        if not self.config.disable_live_feeds:
            self.start_child("live-feeds", self.live_feeds_command())
        if not self.config.disable_build_watch:
            self.start_child("build-watch", self.build_watch_command())
        if not self.config.disable_pipeline_health:
            self.start_child("pipeline-health", self.pipeline_health_command())

    def prepare_dirs(self) -> None:
        for path in [
            self.config.stack_root,
            self.config.live_root,
            self.config.scratch_root,
            self.config.fetch_cache_root,
            self.config.health_root,
            self.config.target_dir,
        ]:
            path.mkdir(parents=True, exist_ok=True)
        self.write_health()

    def build_binaries(self) -> None:
        env = self.child_env()
        command = [
            "cargo",
            "build",
            "--manifest-path",
            str(PREPROCESSOR_MANIFEST),
            "-p",
            "live-feeds-daemon",
        ]
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
        return [
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
                    self.proxy(config.live_feeds_listen, path, parsed.query, send_body)
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
        ) -> None:
            host, port = parse_listen(listen)
            target = path
            if query:
                target = f"{target}?{query}"
            connection = http.client.HTTPConnection(host, port, timeout=3600)
            try:
                connection.request(
                    "HEAD" if not send_body else "GET",
                    target,
                    headers={"Host": listen},
                )
                response = connection.getresponse()
                self.send_response(response.status, response.reason)
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
                    self.send_header(header, value)
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                if send_body:
                    while chunk := response.read(1024 * 64):
                        self.wfile.write(chunk)
                        self.wfile.flush()
            except (ConnectionError, OSError, TimeoutError) as exc:
                self.send_text(502, f"upstream {listen} unavailable: {exc}\n", send_body)
            finally:
                connection.close()

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
    return f"""<!doctype html>
<meta charset="utf-8">
<title>Aerobag Dev Stack</title>
<style>
body {{ margin: 32px; font: 15px/1.45 system-ui, sans-serif; color: #17201b; background: #f7f7f4; }}
main {{ max-width: 880px; }}
a {{ color: #075985; }}
code {{ background: #e7e5df; padding: 1px 4px; border-radius: 4px; }}
li {{ margin: 8px 0; }}
</style>
<main>
  <h1>Aerobag Dev Stack</h1>
  <p>Front door: <code>{display_url(config.listen)}</code></p>
  <ul>
    <li><a href="/pipeline-health/">Pipeline Health</a></li>
    <li><a href="/build-watch/">Build Watch</a></li>
    <li><a href="/live-feeds/status.html">Live-Feed Status</a></li>
    <li><a href="/health.json">Dev Stack Health JSON</a></li>
    <li><a href="/packages/current_artifacts.json">Current Artifacts JSON</a></li>
  </ul>
  <p>Cycle products are served from <code>{config.published_root}</code>.</p>
  <p>Dev live-feed output is isolated at <code>{config.live_root}</code>.</p>
</main>
"""


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
    parser.add_argument("--build-watch-listen", default=DEFAULT_BUILD_WATCH)
    parser.add_argument("--pipeline-health-listen", default=DEFAULT_PIPELINE_HEALTH)
    parser.add_argument("--live-feed-fetch-mode", default="fill", choices=["fill", "offline"])
    parser.add_argument(
        "--web-dist",
        type=Path,
        default=None,
        help="optional built web app dist to serve at /",
    )
    parser.add_argument("--skip-binary-build", action="store_true")
    parser.add_argument("--disable-live-feeds", action="store_true")
    parser.add_argument("--disable-build-watch", action="store_true")
    parser.add_argument("--disable-pipeline-health", action="store_true")
    parser.add_argument("--check-config", action="store_true")
    return parser.parse_args()


def config_from_args(args: argparse.Namespace) -> DevStackConfig:
    artifact_root = args.artifact_root.resolve()
    stack_root = (args.stack_root or artifact_root / "dev-stack").resolve()
    target_dir = (args.target_dir or artifact_root / "target").resolve()
    web_dist = args.web_dist.resolve() if args.web_dist else None
    return DevStackConfig(
        artifact_root=artifact_root,
        stack_root=stack_root,
        web_dist=web_dist,
        target_dir=target_dir,
        listen=args.listen,
        live_feeds_listen=args.live_feeds_listen,
        build_watch_listen=args.build_watch_listen,
        pipeline_health_listen=args.pipeline_health_listen,
        live_feed_fetch_mode=args.live_feed_fetch_mode,
        skip_binary_build=args.skip_binary_build,
        disable_live_feeds=args.disable_live_feeds,
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
                "target_dir": str(config.target_dir),
                "live_feeds_listen": config.live_feeds_listen,
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
