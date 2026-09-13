# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Exercise the installed Rust contract and the real durable publication path."""
from concurrent.futures import ThreadPoolExecutor
import json
import gzip
import http.client
from http.server import ThreadingHTTPServer
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import publish_notices as publisher
import run_dev_stack
sys.path.insert(0, str(publisher.REPO / "product/preprocessor/scripts"))
import pipeline_health


class BulletinPublicationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run([
            "cargo", "build", "--quiet", "--manifest-path",
            str(publisher.REPO / "product/preprocessor/Cargo.toml"),
            "-p", "product-contracts", "--bin", "service-bulletin-contract",
        ], check=True)
        metadata = json.loads(subprocess.check_output([
            "cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path",
            str(publisher.REPO / "product/preprocessor/Cargo.toml"),
        ]))
        cls.validator = Path(metadata["target_directory"]) / "debug/service-bulletin-contract"

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.url = "https://example.test/service/bulletins-v1.json"

    @staticmethod
    def notice(ident="test", revision=1):
        return {"id": ident, "attention_revision": revision, "title": "Service notice",
                "body": "NOTAM distribution quality is suspect; please use caution.",
                "severity": "caution", "published_at_utc": "2026-09-13T00:00:00Z",
                "effective_at_utc": None, "expires_at_utc": None, "resolved": False,
                "audience": {"platforms": [], "releases": []}, "link": None}

    def publish(self, **kwargs):
        return publisher.publish(self.root, self.url, self.validator, **kwargs)

    def test_real_validator_rejects_before_replacing_committed_file(self):
        first = self.publish(notices=[self.notice()])
        committed = (self.root / "bulletins-v1.json").read_bytes()
        bad = self.notice(); bad["link"] = {"label": "click", "url": "javascript:alert(1)"}
        with self.assertRaises(ValueError):
            self.publish(notices=[bad])
        self.assertEqual(committed, (self.root / "bulletins-v1.json").read_bytes())
        same = self.publish(notices=[self.notice()])
        self.assertFalse(same["changed"])
        self.assertEqual(first["sha256"], same["sha256"])
        with self.assertRaises(ValueError):
            self.publish(notices=[self.notice("second")], expected_revision=0)

    def test_monitor_checks_publication_claim_failure_and_document_integrity(self):
        self.publish(notices=[self.notice()])
        collect = lambda: pipeline_health.collect_service_bulletins(self.root, None)
        self.assertIsNone(collect()["error"])
        self.assertEqual(collect()["publication_failure_count"], 0)
        with self.assertRaises(ValueError):
            self.publish(notices=[self.notice(revision=0)])
        failed = collect()
        self.assertEqual(failed["publication_failure_count"], 1)
        metrics = []
        pipeline_health.add_service_bulletin_metrics(metrics, {"inputs":{"service_bulletins":failed}})
        self.assertEqual(metrics[-1]["severity"], "critical")
        self.publish(notices=[self.notice()])
        for mutation in (lambda s: s.pop("publication_failure_count"),
                         lambda s: s["telemetry_contract"].update(sha256="0" * 64)):
            path = self.root / "publication-status.json"
            original = path.read_bytes()
            status = json.loads(original); mutation(status)
            path.write_text(json.dumps(status))
            self.assertIsNotNone(collect()["error"])
            path.write_bytes(original)
        document = publisher.read_document(self.root)
        document["notices"][0]["body"] = "unvalidated edit"
        (self.root / "bulletins-v1.json").write_text(json.dumps(document))
        self.assertIn("receipt", collect()["error"])

    def test_dev_endpoint_compresses_revalidates_and_exposes_no_write_or_private_files(self):
        self.publish(notices=[self.notice()])
        stack = SimpleNamespace(config=SimpleNamespace(service_root=self.root))
        server = ThreadingHTTPServer(("127.0.0.1", 0), run_dev_stack.make_handler(stack))
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        connection = http.client.HTTPConnection(*server.server_address, timeout=3)
        self.addCleanup(connection.close)
        connection.request("GET", publisher.BULLETIN_PATH, headers={"Accept-Encoding":"gzip"})
        response = connection.getresponse(); compressed = response.read()
        self.assertEqual(response.status, 200)
        self.assertEqual(gzip.decompress(compressed), (self.root / "bulletins-v1.json").read_bytes())
        self.assertEqual(response.getheader("Access-Control-Allow-Origin"), "*")
        connection.request("GET", publisher.BULLETIN_PATH, headers={"If-None-Match":response.getheader("ETag")})
        response = connection.getresponse(); self.assertEqual(response.status, 304); self.assertEqual(response.read(), b"")
        for method, path, status in (("POST", publisher.BULLETIN_PATH, 405),
                                    ("GET", "/service/publication-status.json", 404),
                                    ("GET", "/service/history/", 404)):
            connection.request(method, path)
            response = connection.getresponse(); response.read()
            self.assertEqual(response.status, status)

    def test_parallel_section_writers_retain_each_others_changes(self):
        self.publish()
        release = {"release": "release-one", "commit": "a" * 40, "role": "production",
                   "support_until_utc": None, "attention_revision": 1,
                   "web_update_url": "https://example.test/", "android_update_url": "https://example.test/about"}
        with ThreadPoolExecutor(max_workers=3) as pool:
            jobs = [pool.submit(self.publish, notices=[self.notice("a")]),
                    pool.submit(self.publish, notices=[self.notice("b")]),
                    pool.submit(self.publish, releases=[release])]
            revisions = [job.result()["revision"] for job in jobs]
        self.assertEqual(sorted(revisions), [2, 3, 4])
        document = publisher.read_document(self.root)
        self.assertEqual([n["id"] for n in document["notices"]], ["a", "b"])
        self.assertEqual(document["releases"], [release])

    def test_interrupted_commit_and_restart_keep_last_good_publication(self):
        self.publish(notices=[self.notice()])
        committed = (self.root / "bulletins-v1.json").read_bytes()
        original = publisher.atomic_write
        def interrupted(path, data, **kwargs):
            if path.name == "bulletins-v1.json":
                raise OSError("simulated failure before atomic replace")
            original(path, data, **kwargs)
        with patch.object(publisher, "atomic_write", interrupted):
            with self.assertRaises(OSError):
                self.publish(notices=[self.notice("new")])
        self.assertEqual(committed, (self.root / "bulletins-v1.json").read_bytes())
        self.assertEqual(self.publish(notices=[self.notice("new")])["revision"], 2)

    def test_equal_length_replacements_have_distinct_nginx_validators(self):
        self.publish(notices=[self.notice("a")])
        first = (self.root / "bulletins-v1.json").stat()
        item = self.notice("a"); item["body"] = item["body"].replace("suspect", "unclear")
        self.publish(notices=[item])
        second = (self.root / "bulletins-v1.json").stat()
        self.assertNotEqual((int(first.st_mtime), first.st_size), (int(second.st_mtime), second.st_size))

    def test_activated_assignments_not_intent_choose_current_release(self):
        desired = SimpleNamespace(production="new", sunset=[SimpleNamespace(tag="old", until_utc="2026-09-20T00:00:00Z")])
        observed = SimpleNamespace(production="old", staging="new", sunset=[],
                                   releases={"old": SimpleNamespace(commit="a" * 40), "new": SimpleNamespace(commit="b" * 40)})
        self.publish(notices=[self.notice()])
        def reconcile():
            publisher.publish_releases(self.root, self.url, self.validator, desired, observed)
            return {r["release"]: r for r in publisher.read_document(self.root)["releases"]}
        self.assertEqual(reconcile()["old"]["role"], "production")
        observed.production = "new"; observed.staging = None; observed.sunset = ["old"]
        self.assertEqual(reconcile()["old"]["support_until_utc"], "2026-09-20T00:00:00Z")
        observed.sunset = []; del observed.releases["old"]
        self.assertEqual(reconcile()["old"]["role"], "retired")
        self.assertEqual(len(publisher.read_document(self.root)["notices"]), 1)

    def test_lower_attention_revision_and_duplicate_ids_are_rejected(self):
        self.publish(notices=[self.notice(revision=2)])
        with self.assertRaises(ValueError):
            self.publish(notices=[self.notice(revision=1)])
        with self.assertRaises(ValueError):
            self.publish(notices=[self.notice("b"), self.notice("b")])


if __name__ == "__main__":
    unittest.main()
