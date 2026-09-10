# Producer telemetry contracts

Monitoring expectations belong to a deployed producer identity, not to whichever
fields its latest output happens to contain. These contracts are separate from
client data formats such as NAV25, and from monitoring thresholds.

## Authority and storage

- Versioned descriptor files define immutable measurement names, meanings, units,
  types, and applicability. Their raw bytes are SHA256-pinned; never edit an
  existing descriptor, including its formatting. Add a new ID instead.
- `producers.json` selects the contracts promised by the producers in **this
  source revision**. It is a map because different producers evolve independently.
- The release builder reads that file from the selected producer worktree, not
  the controller's current checkout. It copies those pins into `release.json`
  alongside the producer commit and binary hashes.
- The controller's observed release identity and immutable `release.json` select
  monitoring expectations. The publication's `telemetry_contract` claim is
  compared with that expectation; it does not choose or downgrade it.
- The monitor's catalog is installed at `/etc/aerobag/telemetry/`. The helper and
  descriptors are runtime-fingerprinted and installed by `prod_manage --reconcile`.

This protects against a publication dropping fields or advertising an older
contract. It is not a cryptographic defense against an administrator rewriting
both deployment metadata and the catalog; those are trusted deployment inputs.
Missing or mismatched release identity, unknown IDs, wrong digests, and missing
required claims are coverage alarms. None selects a legacy fallback.

## Initial adoption and legacy releases

The first adapter covers the product publisher's `product-facts.json`: product
error counts, warning counts, and NAVDB weather camera site counts. Existing
live-feed, cloud, and controller checks retain their current evaluators; this
change does not claim to have migrated all telemetry sources. Future producer
adapters should use the same independently pinned identity and applicability
gate, with their own payload validation and contract versions.

| Contract | Promises | Publication claim |
| --- | --- | --- |
| `product-facts-v1` | Product error and warning counts | Not emitted by legacy producers |
| `product-facts-v2` | Above, plus unique NAVDB weather camera sites | Not emitted by legacy producers |
| `product-facts-v3` | Same measurements as v2 | Exact contract ID and SHA256 required |

All three use the existing product-facts envelope schema version 1. The telemetry
descriptor, not the envelope schema or NAVDB format, versions these promises.

`legacy-releases.json` is the **one-time frozen bootstrap registry** for all 18
release tags present on September 10, 2026. Bindings use full producer commits
and exact tag names. Inspection of `ProductFactsProduct` and
`product_facts_document` in those revisions established v1 for releases through
`2026-09-07.2`, and v2 for `2026-09-08.1`. The camera telemetry was introduced in
`ed2186bbb814e1d1c467d6a18b2846f6b3fe93e0`; weather cameras themselves predate it.

Do not extend this registry for new releases, infer membership by date, or
rewrite old artifacts. Rebuilding the same old producer does not add newer
instrumentation. New releases must carry explicit pins. Old releases rebuilt by
the new controller receive their original registry pins, not current promises.

## Evaluation and display

Rules declare their required measurements in `PRODUCT_METRIC_REQUIREMENTS`.
The common gate resolves support and validates every applicable product before
allowing the computed metric value to stand:

| Condition | Dashboard result |
| --- | --- |
| Known contract does not promise the measurement | Gray **Not instrumented**, null value, no alarm |
| Promised measurement present and valid | Evaluate operator thresholds normally |
| Promised measurement missing or invalid | Warning on the measurement's own row |
| Contract identity/payload envelope unavailable or inconsistent | Coverage warning; affected measurement rows show gray **Unknown** |

Neutral/unknown values are not recorded as zero or graphed as successful samples.
Only warning and critical metrics enter the alert list. Sunset is not an
exemption: an instrumented sunset release must satisfy its promises just like
production. Monitoring policy still owns thresholds, such as the 960-site
minimum; publishers cannot lower them by sending a threshold field.

Transport/source availability and publication freshness checks remain separate.
This contract mechanism neither suppresses them nor introduces an FAA request
on every monitoring poll. Intentionally disabling a feature is not inferred
from missing data; any future disabled-feature support needs explicit deployment
intent and a reviewed applicability rule.

## Adding or changing telemetry

1. Add a new immutable descriptor with the measurement and precise semantics.
   Reuse an existing measurement ID only if its meaning and applicability stay
   the same. Update the producer pin ID and SHA256 together.
2. Implement emission and register the consuming monitoring rule. The Rust
   product publisher embeds `producers.json` at compilation, so normal publication
   cannot select a different claim via an environment variable or output file.
3. Add tests for valid data, missing/invalid promised data, older unsupported
   contracts, and unknown/mismatched identity. Extend the producer's assertion
   that the embedded pin matches the new descriptor bytes.
4. Advance `coverage-policy.json`'s required baseline when adding coverage. Keep
   thresholds in monitoring policy, outside producer contract declarations.
5. Run `python3 tools/ci/verify_telemetry_contracts.py` and relevant producer/monitor
   tests. The verifier also runs in cheap and staging preflight, and hosted CI.

The verifier compares coverage with the required baseline, the integration base,
and the currently designated production release. Hosted CI supplies the push's
previous SHA or PR base SHA and fetches history/tags; a missing base fails
explicitly. The default `HEAD` checks uncommitted edits locally. Use
`--base-ref COMMIT` to inspect a committed range.

Removing a producer, a required claim, or a measurement (including changing its
meaning or narrowing its family) requires a checked-in exception:

```json
{
  "producer": "product-facts",
  "from": {"id": "old-contract-id", "sha256": "exact-old-digest"},
  "to": {"id": "new-contract-id", "sha256": "exact-new-digest"},
  "measurements": ["exact.sorted.loss.identifiers"],
  "reason": "Why the monitoring loss is intentional and how it is covered",
  "approved_by": "Reviewing operator"
}
```

Put exceptions in `coverage-policy.json`. `to: null` identifies a removed
producer. Exceptions must match exact source/destination pins and the complete
sorted loss list; free-form reasons or wildcard approvals do not bypass the
check. This is an auditable code-review mechanism, not automated proof of human
approval. Never invent an approver to make a check pass.

## Deployment

After committing/pushing this runtime change, `tools/prod_manage.py --reconcile`
can recognize the existing production and sunset contracts without rebuilding
their products or staging an app release. The next normally built release will
pin and emit v3. Do not modify an existing release's `release.json` or delete its
product facts to resolve a coverage alarm.
