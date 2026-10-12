#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import subprocess
import unittest

from tools.admin_index import admin_index_html


class AdminIndexTests(unittest.TestCase):
    def render(self, health: dict) -> dict:
        html = admin_index_html(
            title="Aerobag Test", front_door="https://example.test", commit_hash="abc123",
            cycle_products_root="/artifacts/published", live_feed_output_root="/artifacts/live-feeds/v3",
        )
        # Execute the actual generated page, including its asynchronous fetch and
        # rendering path; source-string checks cannot catch wrong status wiring.
        script = html.split("<script>", 1)[1].split("</script>", 1)[0]
        result = subprocess.run(
            ["node", "--input-type=module", "-e", """
import vm from 'node:vm';
import fs from 'node:fs';
const {script, health} = JSON.parse(fs.readFileSync(0, 'utf8'));
const elements = {services:{}, channels:{}};
vm.runInNewContext(script, {
  document:{getElementById:(id) => elements[id]},
  fetch:async () => ({ok:true, json:async () => health}),
});
await new Promise(setImmediate);
process.stdout.write(JSON.stringify(elements));
"""],
            input=json.dumps({"script": script, "health": health}), text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=10,
        )
        return json.loads(result.stdout)

    @staticmethod
    def channel_health() -> dict:
        return {
            "services": {"provider.service": {"active": "active"}},
            "live_feeds": {"providers": {"provider.service": {
                "provider_tag": "prod", "releases": ["prod", "old"],
            }}},
            "releases": {"production": "prod", "sunset": ["old"], "releases": {
                tag: {"live_feed_status": "pending", "build_status": "passed", "deployment_status": "passed"}
                for tag in ["prod", "old"]
            }},
        }

    def test_idle_successful_job_shows_last_and_next_run(self) -> None:
        result = self.render({"services": {"job.service": {
            "active": "inactive", "enabled": "static", "type": "oneshot",
            "result": "success", "exit_status": "0", "last_finished_at": "Mon 2026-10-12 20:48:10 UTC",
            "timer": "job.timer", "next_run_at_utc": "2026-10-12T22:48:13Z",
        }}})["services"]["innerHTML"]
        self.assertIn('class="ok">Idle — last run succeeded', result)
        self.assertIn("Last finished: Mon 2026-10-12 20:48:10 UTC", result)
        self.assertIn("Next run: 2026-10-12T22:48:13Z", result)
        self.assertIn("static", result)

    def test_idle_is_not_automatically_successful_and_daemons_must_be_active(self) -> None:
        cases = [
            ({"active": "failed", "type": "oneshot", "result": "exit-code"}, "critical", "failed"),
            ({"active": "inactive", "type": "oneshot", "result": "exit-code"}, "critical", "Failed — exit-code"),
            ({"active": "inactive", "type": "oneshot", "result": "success", "exit_status": "0"}, "muted", "Idle — not yet run"),
            ({"active": "inactive", "type": "simple", "result": "success"}, "critical", "inactive"),
            ({"active": "unknown", "error": "unit load state: not-found"}, "critical", "Unknown"),
            ({"active": "activating", "type": "oneshot"}, "warning", "activating"),
            ({"active": "active", "type": "simple"}, "ok", "active"),
            ({"alive": True}, "ok", "active"),
            ({"alive": False, "returncode": 2}, "critical", "failed"),
        ]
        for state, css, label in cases:
            with self.subTest(state=state):
                result = self.render({"services": {"job": state}})["services"]["innerHTML"]
                self.assertIn(f'class="{css}">{label}', result)
                self.assertNotIn("last run succeeded", result)

    def test_schedule_errors_are_visible_and_escaped(self) -> None:
        result = self.render({"services": {"job.timer": {
            "active": "active", "schedule_error": "<unreadable>",
        }}})["services"]["innerHTML"]
        self.assertIn('class="critical">&lt;unreadable&gt;', result)
        self.assertNotIn("not scheduled", result)

    def test_channels_use_activated_provider_not_legacy_or_candidate_status(self) -> None:
        health = self.channel_health()
        health["releases"]["live_feed_instances"] = {"candidate": {"status": "pending"}}
        health["releases"]["releases"]["prod"]["live_feed_provider"] = "candidate"
        result = self.render(health)["channels"]["innerHTML"]
        self.assertIn('class="ok" title="provider.service">Running</dd>', result)
        self.assertIn('class="ok" title="provider.service">Running — shared with prod</dd>', result)
        self.assertNotIn("pending", result)

    def test_provider_runtime_failure_overrides_stale_running_observation(self) -> None:
        for runtime in ["failed", "inactive", "unknown", "activating"]:
            with self.subTest(runtime=runtime):
                health = self.channel_health()
                health["services"]["provider.service"]["active"] = runtime
                health["releases"]["releases"]["prod"]["live_feed_status"] = "running"
                result = self.render(health)["channels"]["innerHTML"]
                self.assertIn(f'>{runtime.capitalize()} — shared with prod</dd>', result)
                self.assertNotIn(">Running", result)

    def test_staging_reports_its_isolated_provider(self) -> None:
        health = self.channel_health()
        health["releases"]["staging"] = "stage"
        health["releases"]["releases"]["stage"] = {"live_feed_status": "pending"}
        health["live_feeds"]["providers"]["stage.service"] = {"provider_tag": "stage", "releases": ["stage"]}
        health["services"]["stage.service"] = {"active": "failed"}
        result = self.render(health)["channels"]["innerHTML"]
        self.assertIn('class="critical" title="stage.service">Failed</dd>', result)
        self.assertIn('class="ok" title="provider.service">Running</dd>', result)

    def test_unknown_ownership_or_runtime_never_falls_back_to_release_status(self) -> None:
        for failure in ["missing-provider", "missing-service", "service-error", "resolution-error", "ambiguous"]:
            with self.subTest(failure=failure):
                health = self.channel_health()
                health["releases"]["releases"]["prod"]["live_feed_status"] = "running"
                if failure == "missing-provider":
                    health["live_feeds"]["providers"] = {}
                elif failure == "missing-service":
                    health["services"] = {}
                elif failure == "service-error":
                    health["services"]["provider.service"]["error"] = "<unreadable>"
                elif failure == "resolution-error":
                    health["live_feeds"]["error"] = "<bad binding>"
                else:
                    health["live_feeds"]["providers"]["second.service"] = {"provider_tag": "prod", "releases": ["prod", "old"]}
                result = self.render(health)["channels"]["innerHTML"]
                self.assertIn(">Unknown", result)
                self.assertNotIn(">Running", result)
                self.assertNotIn("<unreadable>", result)
                self.assertNotIn("<bad binding>", result)

    def test_displays_and_escapes_commit_hash(self) -> None:
        html = admin_index_html(
            title="Aerobag Test",
            front_door="https://example.test",
            commit_hash="abc123<&>",
            cycle_products_root="/artifacts/published",
            live_feed_output_root="/artifacts/live-feeds/v3",
        )

        self.assertIn(
            'Controller commit: <code class="commit">abc123&lt;&amp;&gt;</code>', html
        )

    def test_renders_all_release_roles_from_deployment_health(self) -> None:
        html = admin_index_html(
            title="Aerobag Test",
            front_door="https://example.test",
            commit_hash="abc123",
            cycle_products_root="/artifacts/published",
            live_feed_output_root="/artifacts/live-feeds/v3",
        )

        self.assertIn('fetch("/health.json"', html)
        self.assertIn('card("production"', html)
        self.assertIn('card("staging"', html)
        self.assertIn('card("sunset"', html)
        self.assertIn('/staging/live-feeds/status.html', html)
        self.assertIn('const base = `/releases/${encodeURIComponent(tag)}`', html)
        self.assertIn('live:`${base}/live-feeds/status.html`', html)
        self.assertNotIn('/staging/admin/', html)

    def test_labels_reconciler_checks_without_claiming_full_qualification(self) -> None:
        html = admin_index_html(
            title="Aerobag Test",
            front_door="https://example.test",
            commit_hash="abc123",
            cycle_products_root="/artifacts/published",
            live_feed_output_root="/artifacts/live-feeds/v3",
        )

        self.assertIn("<dt>Deployed checks</dt>", html)
        self.assertIn('"deployment_status" in record ? record.deployment_status : record?.qualification_status', html)
        self.assertIn("stateClass(deployedChecks)", html)
        self.assertIn("esc(record.deployment_error)", html)
        self.assertNotIn("<dt>Qualification</dt>", html)


if __name__ == "__main__":
    unittest.main()
