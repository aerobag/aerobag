# Pipeline Health Alarm Dispositions

Pipeline Health distinguishes measured health from findings which still require
operator action. The downstream jonh.net monitor continues consuming the same
operational status and alert list; it needs no policy change.

## Policy And Semantics

`deploy/alarm-dispositions.json` is version-controlled deployment policy, not a
publisher-controlled field or a change to telemetry contracts. Deployment installs
it at `/etc/aerobag/alarm-dispositions.json` alongside the monitoring engine. Both
files participate in the runtime fingerprint. The normal staging/deployment flow
installs them; an operator can also use `tools/prod_manage.py --reconcile` to update
monitoring without building application releases. Do not deploy automatically
when editing the policy.

Each disposition has an exact metric/finding match, a reason, and evidence:

- `invalid-finding`: the identified result is erroneous, with supporting evidence.
  This is not acceptance of an actual defect. A repeat of the same erroneous
  result stays dispositioned even if an old producer regenerates its report.
- `accepted-on-sunset`: a real, identified defect remains in an explicitly matched
  sunset release, and the operator accepts that remaining exposure. `fixed_by`
  pins a production release tag, full commit, and one or more passing findings.
  Staging is insufficient. All passing evidence must be present in the current
  monitoring sample, raw severity `ok`, and attributed to that production release.
  A rollback, changed producer identity, missing evidence, or failing check
  reactivates the original finding. This is deliberately exact: promoting a
  different fixing release requires updating the policy with its validated checks;
  the monitor does not infer ancestry from tag ordering.

There is no blanket sunset exemption, wildcard match, count-only acknowledgment,
or automatic expiration of an unresolved problem. Invalid policy disables all
dispositions and emits an actionable critical policy alarm. A configured policy
file which cannot be read also alarms; it is not treated as empty policy.

## Adding A Disposition

Inspect `/pipeline-health/current.json`. Eligible metrics expose `finding` and
`finding_match`. Copy the exact `finding_match`, not just a metric name or count:

```json
{
  "schema_version": 1,
  "dispositions": [
    {
      "id": "descriptive-reviewed-issue",
      "kind": "invalid-finding",
      "reason": "Why this specific finding is invalid",
      "evidence": ["Review record and validating check"],
      "match": {
        "metric_id": "exact.metric.id",
        "fingerprint": "<64 lowercase hex characters from finding_match>"
      }
    }
  ]
}
```

For `accepted-on-sunset`, also provide:

```json
"fixed_by": {
  "release_tag": "the-fixing-release",
  "commit": "<full 40-character producer commit>",
  "checks": [{"metric_id": "exact.passing.metric.id", "fingerprint": "<its fingerprint>"}]
}
```

Policy matching is opt-in at the metric evaluator: a rule supplies validated
semantic `finding` evidence before entering the shared disposition engine. The
first adapter is chart-quality completed results. Availability, telemetry-contract
failures, unknown/null measurements, and arbitrary aggregate counts have no such
evidence and cannot be dispositioned. Further adapters must define and test their
own input/subject identity rather than attaching broad suppression rules.

Chart fingerprints include producer/report schema, publication identity when
available, family, cycle, source identity, comparison policy, publication decision,
counts, and the complete non-passing region records (including scores). Release
tag and full producer commit are included when independently bound to a channel.
Only attempt IDs, times, and passing regions are omitted. New inputs, additional
regions, worsening evidence, and changed check semantics fail the match and alarm.
Exact integer-valued floats are normalized for fingerprinting (`0.0`, `-0.0`, and
`0` match); fractional scores are never rounded, booleans and strings remain
distinct, and the raw report is unchanged. Non-finite evidence is ineligible.

During a recheck, the displayed last-completed result retains its own evidence,
cycle, and disposition. This does not disposition the pending attempt: its
separate overdue/invalid-start alarm remains actionable, and the next completed
result must independently match. Missing, malformed, or future-dated completed
evidence cannot authorize a disposition. Schema-3 lookups remain scoped to the
same publication and cycle; a legacy schema-2 result is explicitly unscoped.

## Display And History

Raw metric `severity`, `value`, details, and graph history are unchanged. The
additive `actionable_severity` drives `alerts`, scope statuses, `top_line_status`,
and `overall_status`. Matching findings get a `disposition`; the evaluation also
lists them in `dispositioned_findings`.

The dashboard starts with Known Findings collapsed, separated into Invalidated
Findings and Known Sunset Issues. Links expose the original metric and evidence;
bad measurements are labeled invalidated/accepted, never displayed as passed.
History records both dispositions and a `dispositioned` transition, distinct from
an actual `cleared` result. Removing policy or changing evidence opens the alarm
again; nothing is deleted and there is no sticky acknowledgment state.

## Initial Legacy Findings

The two initial entries cover the Milwaukee inset in SEC and TAC, cycle 2610.
Old schema-2 reports compared it against the cycle-2611 approval. Commit
`bc50eb0ac8db7041d2d66aecee5a05c2bb6ad316` restores cycle-aware selection.
Production `2026-10-11.5-f4fd455d9dd3` completed both checks with zero warnings
against the identical source identities.

`product/preprocessor/scripts/fixtures/chart_quality_legacy_2610.json` captures
the failing and corrected production reports, retaining all non-passing regions,
comparison policies, counts, identities, and times. Passing regions are omitted.
The legacy reports lack release IDs, so only these exact evidence fingerprints
are invalidated. No frozen telemetry registry is extended and no sunset producer
or chart family is broadly exempted.

### Cycle 2611 And Rebuild Replays

On October 13, rebuilding sunset release `2026-09-18.2` replayed its cached
October 10 SEC/TAC reports for cycle 2611. Both named the same Milwaukee inset,
before its human approval in `a7d09db7ee3a01f9ac50714f28582d4eb2dd3c42`.
Production `2026-10-13.1-36edfd8dea27` passes the identical sources. Two additional
exact dispositions cover these pre-approval findings; they do not suppress new
cycles, inputs, or changed findings. Captured original and corrected reports are
in `product/preprocessor/scripts/fixtures/chart_quality_legacy_2611.json`.

The incident also exposed two matching bugs: the original 2610 fixture capture
had rewritten `0.0` scores to `0`, and pending attempts dropped the displayed
completed result's fingerprint. Fixtures now preserve the original numeric
types, fingerprints normalize equivalent numbers, and rechecks retain the
completed evidence. Both fixture files omit passing regions only.

## Tests

`test_alarm_dispositions.py` exercises the real evaluator: actionable status,
raw history preservation, production versus staging, rollback/re-promotion,
missing/malformed evidence, new/worsening findings, bad policy, and the captured
production reports, including collector-to-evaluator recheck transitions and
numeric serialization differences. Deployment tests verify installation and runtime fingerprints.
`smoke_pipeline_health_dashboard.py` exercises collapsed groups, evidence links,
neutral disposition labels, raw metrics, and ordinary alert navigation in Chrome.
