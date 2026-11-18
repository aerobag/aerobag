# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).parent))
import build_release
import client_artifacts
import prod_manage
import publish_notices
import release_reconciler as releases
import service_contract
import test_build_release

ROOT = Path(__file__).resolve().parents[1]


class ServiceContractTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.contract = service_contract.load(ROOT)

    def client(self, name="client"):
        root = self.root / name
        root.mkdir()
        metadata = test_build_release.ReleaseBuildTests()._release_with_declared_inventory(root)
        shutil.copyfile(ROOT / service_contract.CURRENT, root / "service-contract.json")
        metadata.update(schema_version=2, service_contract="C1")
        metadata["artifacts"]["service_contract"] = {
            "filename": "service-contract.json", "sha256": build_release._sha256(root / "service-contract.json"),
        }
        metadata["client"] = {"tag": metadata["tag"], "commit": metadata["commit"],
                              "artifacts": {key: metadata["artifacts"][key] for key in ("web", "downloads", "apk")}}
        (root / "release.json").write_text(json.dumps(metadata))
        return root, metadata

    def test_server_only_preserves_client_bytes_and_rejects_contract_changes(self):
        root, metadata = self.client()
        destination = self.root / "server"
        destination.mkdir()
        client = build_release.copy_client_artifacts(root, destination, self.contract)
        self.assertEqual(client, metadata["client"])
        for kind in ("web", "downloads"):
            self.assertEqual(build_release.directory_sha256(root / kind), build_release.directory_sha256(destination / kind))
        changed = copy.deepcopy(self.contract)
        changed["behavior"].append("different-error-semantics")
        with self.assertRaisesRegex(ValueError, "stage a new client"):
            build_release.retained_client(root, changed)
        (root / "web/index.html").write_text("tampered")
        with self.assertRaises(RuntimeError):
            build_release.retained_client(root, self.contract)

    def test_client_assets_survive_server_retirement_without_retaining_producer(self):
        root, metadata = self.client()
        retained = client_artifacts.retain(self.root / "client-builds", root)
        shutil.rmtree(root)
        self.assertEqual(client_artifacts.validate(retained)["client"], metadata["client"])
        self.assertEqual(client_artifacts.supported(retained.parent, {"C1": self.contract}), {metadata["tag"]: retained})
        self.assertEqual(client_artifacts.supported(retained.parent, {}), {})
        destination = self.root / "next-server"
        destination.mkdir()
        self.assertEqual(build_release.copy_client_artifacts(retained, destination, self.contract), metadata["client"])
        client_artifacts.collect(retained.parent, set(), now=100)
        client_artifacts.collect(retained.parent, {"C1"}, now=4000)
        self.assertTrue(retained.exists())
        client_artifacts.collect(retained.parent, set(), now=4001)
        client_artifacts.collect(retained.parent, set(), now=7602)
        self.assertFalse(retained.exists())

    def test_server_stage_and_promotion_keep_original_client_without_sunset(self):
        document = {"schema_version": 3, "production": {"tag": "server1", "client_tag": "client1"},
                    "staging": None, "sunset": []}
        staged = prod_manage.stage_document(document, "server2", server_only=True)
        self.assertEqual(staged["staging"], {"tag": "server2", "client_tag": "client1"})
        promoted, _, _ = prod_manage.promotion_document(staged, same_contract=True)
        self.assertEqual(promoted["production"], staged["staging"])
        self.assertEqual(promoted["sunset"], [])
        self.assertIsNone(promoted["staging"])
        legacy, _, _ = prod_manage.promotion_document(staged)
        self.assertEqual(legacy["sunset"][0]["tag"], "server1")

    def test_production_and_staging_contract_views_are_isolated_and_reversible(self):
        published = self.root / "published"
        published.mkdir()
        assets, manifests = {}, {}
        for tag, marker in (("server1", "one"), ("server2", "two")):
            root = self.root / tag
            for kind in ("web", "downloads"):
                (root / kind).mkdir(parents=True)
                (root / kind / "asset").write_text("identical client")
            (published / marker).mkdir()
            (published / marker / "payload").write_text(marker)
            assets[tag] = releases.ReleaseAssets(root, "http://127.0.0.1:8000", self.contract, "client1")
            manifests[tag] = releases.ChannelManifest(tag, self.root / f"{tag}.json",
                {"contracts": self.contract["client_data"]["package_contracts"], "marker": marker}, (marker,))
        for index, (production, staging) in enumerate((("server1", "server2"), ("server2", "server1"), ("server1", "server2"))):
            generation = self.root / "channel-generations" / str(index)
            releases.materialize_channel_generation(generation, published,
                production_manifests=[manifests[production]], staging_manifests=[manifests[staging]], release_assets=assets)
            for prefix, tag in (("contracts", production), ("staging/contracts", staging)):
                path = generation / prefix / "C1/packages/current_artifacts.json"
                self.assertEqual(json.loads(path.read_text()), [manifests[tag].document])
            self.assertTrue((generation / "releases/client1/web/asset").exists())
            # Existing release data trees still describe that exact deployment.
            old = json.loads((generation / "releases/server1/packages/current_artifacts.json").read_text())
            self.assertEqual(old[0]["marker"], "one")
        incompatible = copy.deepcopy(self.contract)
        incompatible["behavior"].append("incompatible")
        assets["server2"] = releases.ReleaseAssets(assets["server2"].release_root, None, incompatible, "client1")
        with self.assertRaisesRegex(ValueError, "contracts differ"):
            releases.materialize_channel_generation(self.root / "rejected", published,
                production_manifests=[manifests["server1"]], staging_manifests=[manifests["server2"]], release_assets=assets)
        self.assertFalse((self.root / "rejected/service-bindings.json").exists())

    def test_support_tracks_contract_instead_of_server_or_client_build(self):
        root, _ = self.client()
        observed = SimpleNamespace(production="server1", staging="server2", sunset=[],
            releases={tag: SimpleNamespace(release_root=str(root)) for tag in ("server1", "server2")})
        desired = releases.DesiredReleases(releases.ReleaseBinding("server1"), releases.ReleaseBinding("server2"), ())
        first = publish_notices.contract_support(self.root, "https://example.test", desired, observed)
        self.assertEqual([(entry["contract"], entry["role"], entry["support_until_utc"]) for entry in first], [("C1", "production", None)])
        (self.root / "bulletins-v2.json").write_text(json.dumps({"contracts": first}))
        observed.production, observed.staging = "server2", None
        self.assertEqual(publish_notices.contract_support(self.root, "https://example.test", desired, observed), first)

    def test_contract_routes_proxy_with_sse_and_do_not_alias_legacy_release_urls(self):
        rendered = releases.render_live_feed_nginx_routes(production_endpoint="http://127.0.0.1:8101",
            staging_endpoint="http://127.0.0.1:8102", release_endpoints={},
            contract_endpoints={"C1": "http://127.0.0.1:8101"}, staging_contract_endpoints={"C1": "http://127.0.0.1:8102"})
        self.assertIn("location /contracts/C1/live-feeds/", rendered)
        self.assertIn("location /staging/contracts/C1/live-feeds/", rendered)
        self.assertIn("proxy_buffering off", rendered)
        self.assertNotIn("/releases/", rendered)


if __name__ == "__main__":
    unittest.main()
