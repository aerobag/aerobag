# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Evidence-bound operator decisions, separate from measured health severity."""

import hashlib
import json
import re


def normalize_numbers(value):
    # JSON writers may emit the same score as 0 or 0.0. Normalize only exact
    # integer-valued floats; never round scores or coerce booleans/strings.
    if isinstance(value, float) and value.is_integer():
        return int(value)
    if isinstance(value, dict):
        return {key: normalize_numbers(item) for key, item in value.items()}
    if isinstance(value, list):
        return [normalize_numbers(item) for item in value]
    return value


def canonical_json(value):
    return json.dumps(normalize_numbers(value), sort_keys=True, separators=(",", ":"), allow_nan=False)


def validate_match(value):
    if (not isinstance(value, dict) or set(value) != {"metric_id", "fingerprint"}
            or not isinstance(value["metric_id"], str)
            or not re.fullmatch(r"[A-Za-z0-9_.-]+", value["metric_id"])
            or not isinstance(value["fingerprint"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", value["fingerprint"])):
        raise ValueError("each match requires an exact metric_id and SHA256 fingerprint")


def validate_policy(policy):
    if (not isinstance(policy, dict) or set(policy) != {"schema_version", "dispositions"}
            or type(policy["schema_version"]) is not int or policy["schema_version"] != 1
            or not isinstance(policy["dispositions"], list)):
        raise ValueError("expected alarm disposition policy schema 1")
    ids, matches = set(), set()
    for rule in policy["dispositions"]:
        if not isinstance(rule, dict):
            raise ValueError("disposition must be an object")
        required = {"id", "kind", "reason", "evidence", "match"}
        if rule.get("kind") == "accepted-on-sunset":
            required.add("fixed_by")
        if set(rule) != required or rule.get("kind") not in {"invalid-finding", "accepted-on-sunset"}:
            raise ValueError("invalid disposition kind or fields")
        if (not isinstance(rule["id"], str) or not re.fullmatch(r"[a-z0-9-]+", rule["id"])
                or not isinstance(rule["reason"], str) or not rule["reason"].strip()
                or not isinstance(rule["evidence"], list) or not rule["evidence"]
                or any(not isinstance(e, str) or not e.strip() for e in rule["evidence"])):
            raise ValueError("disposition requires an id, reason, and evidence")
        validate_match(rule["match"])
        match = canonical_json(rule["match"])
        if rule["id"] in ids or match in matches:
            raise ValueError("duplicate disposition id or finding match")
        ids.add(rule["id"])
        matches.add(match)
        if rule["kind"] == "accepted-on-sunset":
            fix = rule["fixed_by"]
            if (not isinstance(fix, dict) or set(fix) != {"release_tag", "commit", "checks"}
                    or not isinstance(fix["release_tag"], str)
                    or not re.fullmatch(r"[A-Za-z0-9_.-]+", fix["release_tag"])
                    or not isinstance(fix["commit"], str)
                    or not re.fullmatch(r"[0-9a-f]{40}", fix["commit"])
                    or not isinstance(fix["checks"], list) or not fix["checks"]):
                raise ValueError("sunset disposition requires a pinned fixing release and passing checks")
            for check in fix["checks"]:
                validate_match(check)


def finding_match(metric):
    # Only rules which supply validated semantic evidence opt in. Availability
    # errors and arbitrary numeric counts are not sufficient evidence to silence.
    finding = metric.get("finding")
    if (not isinstance(finding, dict) or not finding.get("producer")
            or metric.get("value") is None or metric.get("availability") in {"invalid", "unknown"}
            or metric.get("severity") not in {"ok", "warning", "critical"}):
        return None
    evidence = {"finding": finding, "value": metric["value"], "severity": metric["severity"],
                "release_tag": metric.get("release_tag"), "producer_commit": metric.get("producer_commit")}
    return {"metric_id": metric["id"],
            "fingerprint": hashlib.sha256(canonical_json(evidence).encode()).hexdigest()}


def fixing_release_is_production(fix, metrics, facts):
    inputs = facts.get("inputs", {})
    if any(inputs.get(key, {}).get("error") for key in ("release_channels", "release_state")):
        return False
    production = [c for c in facts.get("channels", {}).values()
                  if isinstance(c, dict) and c.get("role") == "production"]
    if (len(production) != 1 or production[0].get("tag") != fix["release_tag"]
            or (production[0].get("release_state") or {}).get("commit") != fix["commit"]):
        return False
    for check in fix["checks"]:
        passing = [m for m in metrics if m.get("finding_match") == check and m.get("severity") == "ok"
                   and m.get("release_tag") == fix["release_tag"]
                   and m.get("producer_commit") == fix["commit"]
                   and m.get("channel_role") == "production"]
        if len(passing) != 1:
            return False
    return True


def apply(metrics, facts):
    """Annotate in place, returning a policy error without silencing anything."""
    for metric in metrics:
        metric.pop("disposition", None)
        metric.pop("finding_match", None)
        metric["actionable_severity"] = metric.get("severity", "ok")
        try:
            match = finding_match(metric)
        except (TypeError, ValueError):
            # Invalid evidence is never eligible for an operator disposition.
            match = None
        if match is not None:
            metric["finding_match"] = match
    source = facts.get("inputs", {}).get("alarm_dispositions")
    if source is None:
        return None
    if source.get("error"):
        return str(source["error"])
    policy = source.get("payload")
    try:
        validate_policy(policy)
    except ValueError as error:
        return str(error)
    rules = {canonical_json(rule["match"]): rule for rule in policy["dispositions"]}
    for metric in metrics:
        rule = rules.get(canonical_json(metric.get("finding_match")))
        if rule is None or metric.get("severity") not in {"warning", "critical"}:
            continue
        if rule["kind"] == "accepted-on-sunset" and (
                metric.get("channel_role") != "sunset" or not metric.get("producer_commit")
                or not fixing_release_is_production(rule["fixed_by"], metrics, facts)):
            continue
        metric["disposition"] = rule
        metric["actionable_severity"] = "ok"
    return None
