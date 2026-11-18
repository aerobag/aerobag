# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Real nginx integration: contract publication/provider switch and rollback."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import unittest
from http.client import HTTPConnection

import release_reconciler as releases
import service_contract


@unittest.skipUnless(shutil.which("nginx"), "real nginx is required for proxy integration")
class ContractProxyTests(unittest.TestCase):
    def test_static_and_live_routes_switch_together_with_staging_isolated(self):
        with tempfile.TemporaryDirectory(prefix="aerobag-contract-proxy-") as temporary:
            root = Path(temporary)
            root.chmod(0o755)
            servers = []
            for marker in ("old", "candidate"):
                class Handler(BaseHTTPRequestHandler):
                    identity = marker
                    def do_GET(self):
                        raw = json.dumps({"provider": self.identity, "path": self.path}).encode()
                        self.send_response(200)
                        self.send_header("Content-Type", "application/json")
                        self.send_header("Content-Length", str(len(raw)))
                        self.end_headers()
                        self.wfile.write(raw)
                    def log_message(self, *_args):
                        pass
                server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
                threading.Thread(target=server.serve_forever, daemon=True).start()
                self.addCleanup(server.server_close)
                self.addCleanup(server.shutdown)
                servers.append(f"http://127.0.0.1:{server.server_port}")
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            contract = service_contract.load(Path(__file__).resolve().parents[1])
            routes = root / "routes.conf"
            configuration = root / "nginx.conf"
            configuration.write_text(f"""
daemon off;
master_process on;
pid {root}/nginx.pid;
error_log {root}/error.log;
events {{ worker_connections 32; }}
http {{ access_log off; default_type application/json;
  client_body_temp_path {root}/body;
  proxy_temp_path {root}/proxy;
  fastcgi_temp_path {root}/fastcgi;
  uwsgi_temp_path {root}/uwsgi;
  scgi_temp_path {root}/scgi;
  server {{ listen 127.0.0.1:{port}; include {routes}; }}
}}
""")
            def prepare(number, prod, stage):
                generation = root / str(number)
                for prefix, provider in (("contracts", prod), ("staging/contracts", stage)):
                    directory = generation / prefix / "C1"
                    (directory / "packages").mkdir(parents=True)
                    (directory / "packages/current_artifacts.json").write_text(json.dumps({"provider": provider}))
                    (directory / "contract.json").write_text(json.dumps(contract))
                routes.write_text(releases.render_live_feed_nginx_routes(
                    production_endpoint=servers[prod], staging_endpoint=servers[stage], release_endpoints={},
                    contract_endpoints={"C1": servers[prod]}, staging_contract_endpoints={"C1": servers[stage]},
                    contract_generation=generation))
            connection = None
            def get(path):
                connection.request("GET", path)
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                return json.loads(response.read())
            prepare(0, 0, 1)
            process = subprocess.Popen([shutil.which("nginx"), "-p", str(root), "-c", str(configuration)],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            try:
                for number, (prod, stage) in enumerate(((0, 1), (1, 0), (0, 1))):
                    if number:
                        prepare(number, prod, stage)
                        process.send_signal(__import__("signal").SIGHUP)
                    deadline = time.monotonic() + 5
                    while True:
                        if connection is not None:
                            connection.close()
                        connection = HTTPConnection("127.0.0.1", port, timeout=2)
                        try:
                            ready = get("/contracts/C1/packages/current_artifacts.json")["provider"] == prod
                            if ready:
                                break
                        except OSError:
                            if process.poll() is not None:
                                self.fail(process.stderr.read().decode())
                        if time.monotonic() >= deadline:
                            self.fail("nginx did not activate the expected contract generation")
                        time.sleep(0.02)
                    for prefix, provider in (("/contracts", prod), ("/staging/contracts", stage)):
                        self.assertEqual(get(prefix + "/C1/packages/current_artifacts.json")["provider"], provider)
                        live = get(prefix + "/C1/live-feeds/current.json?cursor=42")
                        self.assertEqual(live["provider"], ("old", "candidate")[provider], f"phase={number} prefix={prefix}")
                        self.assertEqual(live["path"], "/live-feeds/current.json?cursor=42")
                        self.assertEqual(get(prefix + "/C1/contract.json"), contract)
            finally:
                if connection is not None:
                    connection.close()
                process.terminate()
                process.communicate(timeout=5)
