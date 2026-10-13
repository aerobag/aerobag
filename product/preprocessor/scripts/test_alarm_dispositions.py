# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

import copy
import json
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pipeline_health as health


class AlarmDispositionTests(unittest.TestCase):
    now = datetime(2026, 10, 13, 14, tzinfo=timezone.utc)

    def report(self, publication=None, status="warning"):
        report = {
            "schema_version": 3 if publication else 2, "family": "SEC", "cycle": "2610",
            "source_id": "source-a", "report_id": "a" * 32,
            "started_at": "2026-10-11T02:00:00Z", "completed_at": "2026-10-11T02:01:00Z",
            "status": status, "warning_count": int(status == "warning"),
            "critical_count": 0, "unreviewed_count": 0,
            "publication": {"state": "ready", "has_map_sources": True,
                            "quarantined_sources": [], "excluded_metadata": []},
            "regions": [{"id": "region-a", "chart": "Chicago SEC", "name": "Milwaukee",
                         "status": status, "scores": {"control": {"control": .08}}}],
        }
        if publication:
            report["publication_id"] = publication
        return report

    def facts(self, reports):
        return {"inputs": {"build_watch": {"payload": {"result": {"status": "pass"}}},
                           "chart_quality": [{"family": "SEC", "payload": r} for r in reports]},
                "channels": {}}

    def evaluate(self, facts):
        return health.evaluate_health(facts, [], self.now)

    def row(self, result, publication=None):
        return next(m for m in result["metrics"] if m["id"].endswith(".unresolved")
                    and m["details"]["publication_id"] == publication)

    def rule(self, row, kind="invalid-finding"):
        return {"id": "review-2610", "kind": kind,
                "reason": "Wrong cycle approval compared; verified with cycle-aware checker.",
                "evidence": ["commit bc50eb0a; reviewed production reports on 2026-10-12"],
                "match": row["finding_match"]}

    def install_policy(self, facts, rules):
        facts["inputs"]["alarm_dispositions"] = {
            "payload": {"schema_version": 1, "dispositions": rules}, "error": None}

    def captured_reports(self, cycle):
        return json.loads((Path(__file__).parent / f"fixtures/chart_quality_legacy_{cycle}.json").read_text())

    def deployed_policy(self):
        return json.loads((Path(__file__).resolve().parents[3] / "deploy/alarm-dispositions.json").read_text())

    def test_numeric_representation_does_not_change_fingerprint_or_raw_evidence(self):
        metric = {"id": "example.content", "value": 1, "severity": "warning",
                  "finding": {"producer": "example", "scores": [0, {"nested": [1, 2.5]}]}}
        facts = self.facts([])
        health.alarm_dispositions.apply([metric], facts)
        self.install_policy(facts, [self.rule(metric)])
        expected = metric["finding_match"]
        for zero in (0.0, -0.0):
            changed = copy.deepcopy(metric)
            changed["value"] = 1.0
            changed["finding"]["scores"] = [zero, {"nested": [1.0, 2.5]}]
            raw = json.dumps(changed["finding"])
            health.alarm_dispositions.apply([changed], facts)
            self.assertEqual(changed["finding_match"], expected)
            self.assertEqual(changed["actionable_severity"], "ok")
            self.assertEqual(json.dumps(changed["finding"]), raw)

    def test_numeric_normalization_does_not_merge_distinct_evidence(self):
        canonical = health.alarm_dispositions.canonical_json
        for left, right in [(False, 0), (True, 1), ("0", 0), (None, 0),
                            (0, 1e-100), (1, 1.0000000000000002),
                            (9007199254740992, 9007199254740993)]:
            with self.subTest(left=left, right=right):
                self.assertNotEqual(canonical({"score": left}), canonical({"score": right}))

    def test_invalid_finding_leaves_raw_evidence_but_not_downstream_alarm(self):
        facts = self.facts([self.report()])
        before = self.evaluate(facts)
        row = self.row(before)
        self.assertEqual(before["top_line_status"], "warning")
        self.install_policy(facts, [self.rule(row)])
        after = self.evaluate(facts)
        current = self.row(after)
        self.assertEqual(current["severity"], "warning")
        self.assertEqual(current["value"], 1)
        self.assertEqual(current["details"], row["details"])
        self.assertEqual(current["actionable_severity"], "ok")
        self.assertEqual(after["top_line_status"], "ok")
        self.assertEqual(after["overall_status"], "ok")
        self.assertEqual(after["alerts"], [])
        self.assertEqual(after["dispositioned_findings"][0]["disposition"]["id"], "review-2610")
        history = health.compact_history_record({"sampled_at_utc": "2026-10-12T00:00:00Z"}, after,
                                               {"active_alerts": health.compact_active_alerts(before)})
        self.assertEqual(history["metrics"][row["id"]]["severity"], "warning")
        self.assertEqual(history["alert_transitions"][0]["event"], "dispositioned")
        self.assertEqual(history["dispositions"][row["id"]]["id"], "review-2610")
        self.assertEqual(health.compact_existing_history_record(history), history)

    def test_recreated_report_matches_but_new_evidence_reopens(self):
        facts = self.facts([self.report()])
        self.install_policy(facts, [self.rule(self.row(self.evaluate(facts)))])
        repeated = copy.deepcopy(facts)
        repeated["inputs"]["chart_quality"][0]["payload"].update(
            report_id="b" * 32, started_at="2026-10-12T00:00:00Z", completed_at="2026-10-12T00:01:00Z")
        self.assertEqual(self.evaluate(repeated)["top_line_status"], "ok")
        for field, value in [("source_id", "source-b"), ("cycle", "2611"),
                             ("schema_version", 3), ("warning_count", 2)]:
            with self.subTest(field=field):
                changed = copy.deepcopy(facts)
                changed["inputs"]["chart_quality"][0]["payload"][field] = value
                self.assertNotEqual(self.evaluate(changed)["top_line_status"], "ok")
        changed = copy.deepcopy(facts)
        changed["inputs"]["chart_quality"][0]["payload"]["regions"][0]["scores"]["control"]["control"] = .09
        self.assertEqual(self.evaluate(changed)["top_line_status"], "warning")
        for field in ["completed_at", "source_id"]:
            changed = copy.deepcopy(facts)
            changed["inputs"]["chart_quality"][0]["payload"].pop(field)
            self.assertEqual(self.evaluate(changed)["top_line_status"], "warning")
        changed = copy.deepcopy(facts)
        changed["inputs"]["chart_quality"][0]["payload"]["completed_at"] = "2027-01-01T00:00:00Z"
        self.assertEqual(self.evaluate(changed)["top_line_status"], "warning")

    def test_other_findings_and_missing_or_malformed_reports_still_alarm(self):
        facts = self.facts([self.report()])
        self.install_policy(facts, [self.rule(self.row(self.evaluate(facts)))])
        other = self.report("other-producer")
        facts["inputs"]["chart_quality"].append({"family": "SEC", "payload": other})
        self.assertEqual(len(self.evaluate(facts)["alerts"]), 1)
        for entry in [{"family": "SEC", "payload": None, "error": "missing file"},
                      {"family": "SEC", "payload": {**self.report(), "warning_count": None}},
                      {"family": "SEC", "payload": self.report("checking") | {"status": "checking"}}]:
            changed = copy.deepcopy(facts)
            changed["inputs"]["chart_quality"] = [entry]
            result = self.evaluate(changed)
            self.assertNotEqual(result["top_line_status"], "ok")
            self.assertEqual(result["dispositioned_findings"], [])

    def sunset_facts(self):
        old, new = "old-" + "a" * 12, "new-" + "b" * 12
        facts = self.facts([self.report(old), self.report(new, "ok")])
        facts["channels"] = {"release-old": {"tag": "old", "role": "sunset",
                                             "release_state": {"commit": "a" * 40}},
                             "production": {"tag": "new", "role": "production",
                                            "release_state": {"commit": "b" * 40}}}
        result = self.evaluate(facts)
        rule = self.rule(self.row(result, old), "accepted-on-sunset")
        rule["fixed_by"] = {"release_tag": "new", "commit": "b" * 40,
                            "checks": [self.row(result, new)["finding_match"]]}
        self.install_policy(facts, [rule])
        return facts

    def test_sunset_acceptance_requires_promoted_fix_and_current_passing_evidence(self):
        facts = self.sunset_facts()
        self.assertEqual(self.evaluate(facts)["top_line_status"], "ok")
        changes = [
            lambda f: f["channels"]["production"].update(role="staging"),
            lambda f: f["channels"]["production"]["release_state"].update(commit="c" * 40),
            lambda f: f["channels"]["release-old"].update(role="production"),
            lambda f: f["inputs"]["chart_quality"].pop(),
            lambda f: f["inputs"]["chart_quality"][1]["payload"].update(status="warning", warning_count=1),
            lambda f: f["inputs"]["chart_quality"][1]["payload"].update(status="checking"),
            lambda f: f["inputs"]["chart_quality"][1]["payload"].update(source_id="another-source"),
            lambda f: f["inputs"].update(release_state={"error": "unavailable"}),
        ]
        for change in changes:
            altered = copy.deepcopy(facts)
            change(altered)
            with self.subTest(change=change):
                self.assertNotEqual(self.evaluate(altered)["top_line_status"], "ok")
                self.assertEqual(self.evaluate(altered)["dispositioned_findings"], [])

    def test_rollback_and_repromotion_recompute_disposition_without_sticky_state(self):
        facts = self.sunset_facts()
        self.assertEqual(self.evaluate(facts)["top_line_status"], "ok")
        facts["channels"]["production"]["role"] = "sunset"
        facts["channels"]["release-old"]["role"] = "production"
        self.assertEqual(self.evaluate(facts)["top_line_status"], "warning")
        facts["channels"]["production"]["role"] = "production"
        facts["channels"]["release-old"]["role"] = "sunset"
        self.assertEqual(self.evaluate(facts)["top_line_status"], "ok")

    def test_malformed_or_missing_config_disables_all_dispositions_and_alarms(self):
        facts = self.facts([self.report()])
        rule = self.rule(self.row(self.evaluate(facts)))
        invalid_rules = [dict(rule, match={"metric_id": "chart_quality.*", "fingerprint": "0" * 64}),
                         dict(rule, kind="mute"), dict(rule, reason=""), dict(rule, evidence=[]),
                         dict(rule, surprise=True), dict(rule, kind="accepted-on-sunset")]
        for bad in invalid_rules:
            with self.subTest(bad=bad):
                self.install_policy(facts, [rule, bad])
                result = self.evaluate(facts)
                self.assertEqual(result["dispositioned_findings"], [])
                self.assertEqual(result["top_line_status"], "critical")
        facts["inputs"]["alarm_dispositions"] = {"payload": None, "error": "missing file"}
        self.assertEqual(self.evaluate(facts)["top_line_status"], "critical")

    def test_checked_in_policy_is_narrow_and_documented(self):
        policy = self.deployed_policy()
        health.alarm_dispositions.validate_policy(policy)
        self.assertEqual({r["id"] for r in policy["dispositions"]}, {
            "sec-2610-milwaukee-wrong-cycle-reference", "tac-2610-milwaukee-wrong-cycle-reference",
            "sec-2611-milwaukee-before-approval", "tac-2611-milwaukee-before-approval"})
        self.assertEqual({r["match"]["metric_id"] for r in policy["dispositions"]},
                         {"chart_quality.SEC.unresolved", "chart_quality.TAC.unresolved"})
        self.assertTrue(all(r["kind"] == "invalid-finding" for r in policy["dispositions"]))

    def test_real_production_reports_match_policy_without_hiding_new_findings(self):
        for cycle in ("2610", "2611"):
            with self.subTest(cycle=cycle):
                captured = self.captured_reports(cycle)
                facts = self.facts([])
                facts["inputs"]["chart_quality"] = [{"family": r["family"], "payload": r}
                                                     for r in captured["legacy"] + captured["corrected"]]
                self.assertEqual(len(self.evaluate(facts)["alerts"]), 2)
                self.install_policy(facts, self.deployed_policy()["dispositions"])
                result = self.evaluate(facts)
                self.assertEqual(result["top_line_status"], "ok")
                self.assertEqual(len(result["dispositioned_findings"]), 2)
                for old, fixed in zip(captured["legacy"], captured["corrected"]):
                    self.assertEqual((old["family"], old["cycle"], old["source_id"]),
                                     (fixed["family"], fixed["cycle"], fixed["source_id"]))
                    self.assertEqual(fixed["status"], "ok")
                    self.assertIsInstance(old["regions"][0]["scores"]["control-2"]["control"], float,
                                          "keep the original report's numeric representation")
                captured["legacy"][0]["regions"].append({"id": "new-finding", "status": "warning", "scores": {}})
                captured["legacy"][0]["warning_count"] += 1
                result = self.evaluate(facts)
                self.assertEqual(len(result["alerts"]), 1)
                self.assertEqual(result["alerts"][0]["metric_id"], "chart_quality.SEC.unresolved")

    def test_disposition_survives_in_progress_check_but_not_new_or_overdue_findings(self):
        for cycle in ("2610", "2611"):
            for report in self.captured_reports(cycle)["legacy"]:
                with self.subTest(cycle=cycle, family=report["family"]), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    family = root / "state/chart-quality" / report["family"]
                    completed = family / "reports" / report["report_id"] / "report.json"
                    completed.parent.mkdir(parents=True)
                    completed.write_text(json.dumps(report))
                    current = family / "current.json"
                    current.write_text(json.dumps(report))
                    facts = self.facts([])
                    # Derive this rule from the exact report to isolate retry handling
                    # from the separately tested deployed policy and numeric matching.
                    facts["inputs"]["chart_quality"] = health.collect_chart_quality(root)
                    before = self.row(self.evaluate(facts))
                    self.install_policy(facts, [self.rule(before)])
                    pending = {**report, "status": "checking", "report_id": "b" * 32,
                               "started_at": self.now.isoformat(), "cycle": "2612", "source_id": "new-source"}
                    pending.pop("completed_at")
                    current.write_text(json.dumps(pending))
                    facts["inputs"]["chart_quality"] = health.collect_chart_quality(root)
                    result = self.evaluate(facts)
                    previous = self.row(result)
                    self.assertEqual(result["alerts"], [])
                    self.assertEqual(previous["finding_match"], before["finding_match"])
                    self.assertEqual(previous["details"], before["details"])
                    self.assertEqual(previous["severity"], "warning")
                    self.assertEqual(previous["actionable_severity"], "ok")

                    pending["started_at"] = "2026-10-12T00:00:00Z"
                    current.write_text(json.dumps(pending))
                    facts["inputs"]["chart_quality"] = health.collect_chart_quality(root)
                    overdue = self.evaluate(facts)
                    self.assertEqual([a["metric_id"] for a in overdue["alerts"]],
                                     [f"chart_quality.{report['family']}.attempt"])
                    self.assertEqual(self.row(overdue)["actionable_severity"], "ok")

                    changed = copy.deepcopy(report)
                    changed["regions"][0]["scores"]["control-1"]["control"] += .01
                    current.write_text(json.dumps(changed))
                    facts["inputs"]["chart_quality"] = health.collect_chart_quality(root)
                    self.assertEqual([a["metric_id"] for a in self.evaluate(facts)["alerts"]],
                                     [f"chart_quality.{report['family']}.unresolved"])

    def test_in_progress_check_cannot_disposition_invalid_previous_evidence(self):
        facts = self.facts([self.report()])
        self.install_policy(facts, [self.rule(self.row(self.evaluate(facts)))])
        pending = {**self.report(), "status": "checking", "started_at": self.now.isoformat()}
        for update in ({"completed_at": None}, {"completed_at": "2027-01-01T00:00:00Z"},
                       {"source_id": None}, {"warning_count": None}, {"status": "checking"}):
            with self.subTest(update=update):
                facts["inputs"]["chart_quality"] = [{"family": "SEC", "payload": pending,
                                                     "last_completed": {**self.report(), **update}}]
                result = self.evaluate(facts)
                self.assertEqual(result["dispositioned_findings"], [])
                row = next(m for m in result["metrics"] if m["id"] == "chart_quality.SEC.unresolved")
                self.assertNotIn("finding_match", row)

    def test_engine_is_shared_but_metrics_without_semantic_evidence_cannot_be_muted(self):
        metric = {"id": "example.content", "value": 1, "severity": "warning",
                  "finding": {"producer": "example", "source_id": "data-v1", "subject": "record-42"}}
        facts = self.facts([])
        health.alarm_dispositions.apply([metric], facts)
        self.install_policy(facts, [self.rule(metric)])
        health.alarm_dispositions.apply([metric], facts)
        self.assertEqual(metric["severity"], "warning")
        self.assertEqual(metric["actionable_severity"], "ok")
        metric.pop("finding")
        health.alarm_dispositions.apply([metric], facts)
        self.assertEqual(metric["actionable_severity"], "warning")
        self.assertNotIn("disposition", metric)

    def test_coverage_error_invalidates_previously_valid_semantic_evidence(self):
        metric = {"id": "example.content", "value": 1, "severity": "warning",
                  "finding": {"producer": "example", "source_id": "v1"}}
        facts = self.facts([])
        health.alarm_dispositions.apply([metric], facts)
        self.install_policy(facts, [self.rule(metric)])
        metric.update(availability="invalid", value=None)
        health.alarm_dispositions.apply([metric], facts)
        self.assertEqual(metric["actionable_severity"], "warning")
        self.assertNotIn("disposition", metric)

    def test_nonfinite_evidence_cannot_crash_or_silence_monitor(self):
        metric = {"id": "example.content", "value": 1, "severity": "warning",
                  "finding": {"producer": "example", "score": float("nan")}}
        health.alarm_dispositions.apply([metric], self.facts([]))
        self.assertEqual(metric["actionable_severity"], "warning")
        self.assertNotIn("finding_match", metric)


if __name__ == "__main__":
    unittest.main()
