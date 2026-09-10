# Live-feed operational status v3

`/live-feeds/status.json` has its own envelope version, independent of client
live-feed payload contracts. Version 3 changes the following operational
measurements; existing fields and bounded attempt/update histories remain.

- Every product must include `failure_episodes`, an object keyed by `source`
  and/or `publication`. An empty object means no known ongoing failure. Missing
  or invalid instrumentation is an error, never an empty object.
- An episode contains RFC3339 `first_failure_at_utc` and `last_failure_at_utc`,
  positive integer `failure_count`, and strings `phase` and `error`. Additional
  failures update the latter fields without resetting the first timestamp.
  Episode state is independent of the bounded attempt history and lasts for
  this daemon process's lifetime or until successful recovery.
- `source` covers the independent collector/auxiliary worker. Only its success
  clears that episode. `publication` covers a product worker's poll, build,
  publication, announcement, acknowledgement, and maintenance. Only a complete
  successful tick clears it. Publishing successfully but failing maintenance
  must not reset the episode timer.
- `consecutive_failure_count` is the sum of unresolved episodes' failure counts.
- For published products, `last_success_at_utc` and
  `last_source_timestamp_utc` describe successful publication, including an
  unchanged product whose collection timestamp advanced. Collector success
  alone does not update them. Source-only auxiliary workers retain source
  success semantics. Raw collector progress remains in source attempt samples.

The monitor supports the already-deployed v2 envelope separately: it reconstructs
source/publication episodes from timestamped attempts and gets publication
freshness from publication successes, not collector heartbeats. V2 cannot
preserve an episode's exact start beyond its bounded history; stale-publication
checks remain independent. V3 does not fall back to v2 when a field is missing.
Unknown envelope versions are monitoring coverage errors.

These live-feed status envelopes are not yet part of the release-pinned
`product-facts` telemetry adapter described in
[producer telemetry contracts](../../contracts/telemetry/README.md). This is an
explicit operational status version, not a claim that all telemetry producers
have been migrated to that catalog.
