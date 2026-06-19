---
id: TASK-67
title: Monitor preprocessing pipeline health
state: high
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - preprocessor
  - deployment
  - data
  - cat:productionization
dependencies: []
ordinal: 67000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add health monitoring for the preprocessing pipeline: whether cycle products and
live-feed products arrive on time, latency, gaps, failure rates, diagnostic
surprises, and enough data to tune poll periods. The monitoring system should be
machine-readable first, with an HTML dashboard layered on top for human use.

The implementation should keep a hard separation between facts and conclusions:

- Product publishers emit only absolute facts: this was attempted at X, fetched
  at Y, published at Z, source timestamp was S, product version was V, warning
  count was N, error count was M.
- The monitor daemon owns history and derives rates, ages, lateness, deltas from
  previous cycle/version, threshold state, and the top-line health summary.
- Raw history records must not bake in warning/critical thresholds or derived
  conclusions such as "late_seconds" or "failure_rate". Those values belong in
  the monitor's evaluated current status.

## Current State

Production currently exposes these relevant endpoints:

- `/health.json`: deploy/service/current-artifacts/live-feed freshness summary.
- `/build-watch/`: human build-progress page from `watch_build_log.py`.
- `/build-watch/api/state`: machine-readable latest build-progress state.
- `/packages/current_artifacts.json`: current published cycle artifacts.
- `/packages/<branch>/<timestamp>/packaged/build-status.html`: per-publication
  human cycle status.
- `/packages/<branch>/<timestamp>/packaged/build_errors_YYYYMMDD.json`:
  per-publication diagnostic errors gathered today.
- `/live-feeds/status.html`: human live-feed status page.
- `/live-feeds/status.json`: machine-readable live-feed sample history.
- `/live-feeds/current.json`: current live-feed product versions.
- `/live-feeds/events`: SSE stream for clients.

Known gaps:

- Live-feed tick results know poll/build/publish/announce failures, but the
  daemon only logs failures to stderr. `/live-feeds/status.json` currently
  records changed publishes, not attempts, unchanged successes, failures, or
  consecutive failure counts.
- The interval source marks a product as polled before fetch/build/publish
  completes. A failed poll then waits the normal interval. There is no
  failure-specific fast retry burst.
- `/health.json` reports service state and current-artifacts freshness, but does
  not promote the latest build-watch failure into an overall unhealthy state.
- Build-watch can report `result=fail` and `pid dead` while still showing tasks
  that were active when the failed run stopped. Consumers should key off
  `result.status`, not active task count.
- Some cycle diagnostics are collected today, especially vector diagnostics, but
  interior validator warnings/errors such as procedure-geometry surprises are
  not consistently promoted into top-level product facts.
- NEXRAD palettization quality is tested/encoded in the producer, but poor color
  match is not exposed as a runtime quality fact.
- Detecting a missing future FAA cycle cannot rely on publisher output. If the
  publisher stops discovering/publishing a new cycle, the monitor still needs to
  know the expected FAA cycle independently.

## Design

### Cycle Publisher Facts

Cycle production should publish a compact fact document for each publication,
next to the packaged artifacts. Suggested path:

`/packages/<branch>/<timestamp>/packaged/product-facts.json`

The file should contain facts only. Example shape:

```json
{
  "schema_version": 1,
  "generated_at_utc": "2026-06-19T03:59:09Z",
  "build": {
    "status": "pass",
    "started_at_utc": "2026-06-19T03:20:00Z",
    "completed_at_utc": "2026-06-19T03:59:09Z"
  },
  "products": [
    {
      "product_id": "NAV_DB_NAV9_2606_01",
      "family": "nav-db",
      "cycle": "2606",
      "cycle_version": "01",
      "effective_date": "2026-06-11",
      "source_fetched_at_utc": "2026-06-19T03:21:00Z",
      "published_at_utc": "2026-06-19T03:58:50Z",
      "error_count": 0,
      "warning_count": 4,
      "diagnostics": {
        "procedure_geometry_warning_count": 4,
        "procedure_geometry_error_count": 0,
        "vector_validator_error_count": 0
      }
    }
  ]
}
```

Do not put derived values in this file: no failure rates, no stale seconds, no
late seconds, no threshold labels. The monitor computes those using its own
clock and history.

Cycle diagnostics to promote into facts should include at least:

- Product-level build error count.
- Product-level build warning count.
- Procedure-geometry validator warning/error counts.
- Vector/HAD diagnostic warning/error counts.
- Other explicit validator violations that would surprise users or make data
  suspect.

The existing `build_errors_YYYYMMDD.json` can continue to exist as detailed
diagnostic payload. `product-facts.json` should provide the scalar counts the
monitor can consume without parsing every detail payload.

### Live-Feed Publisher Facts

Extend `/live-feeds/status.json` so each product exposes first-class attempt,
success, failure, and quality facts. Keep the existing sample history, but add
status fields that answer "what happened most recently?" without scraping logs.

Example success fact:

```json
{
  "product": "metars",
  "version": "a7e61f4e659f5b3b",
  "attempted_at_utc": "2026-06-19T18:54:16Z",
  "published_at_utc": "2026-06-19T18:54:17Z",
  "source_timestamp_utc": "2026-06-19T18:50:00Z",
  "error_count": 0,
  "warning_count": 0
}
```

Example failure fact:

```json
{
  "product": "metars",
  "attempted_at_utc": "2026-06-19T18:59:16Z",
  "failed_at_utc": "2026-06-19T18:59:18Z",
  "phase": "fetch",
  "error": "curl failed for https://aviationweather.gov/..."
}
```

Needed live-feed status fields per product:

- `nominal_interval_seconds`
- `last_attempt_at_utc`
- `last_success_at_utc`
- `last_published_at_utc`
- `last_source_timestamp_utc`
- `last_failure_at_utc`
- `last_failure_phase`
- `last_error`
- `current_version`
- `current_error_count`
- `current_warning_count`
- recent attempt samples, including successes, unchanged successes, and failures

Do not have the daemon emit derived rates or threshold severities. The monitor
has history and computes rates.

For NEXRAD, expose quality facts from the producer, for example:

```json
{
  "product": "nexrad",
  "version": "20260619T185641Z_699f92997e0ff0c3_png89c0edbec",
  "published_at_utc": "2026-06-19T18:57:16Z",
  "quality": {
    "palette_error_max": 4.2,
    "palette_error_p95": 1.7,
    "poor_color_match_count": 0
  }
}
```

### Live-Feed Retry Behavior

Network failures should trigger faster retries, at least for a short burst.
The current implementation marks interval products as polled before the fetch
and then waits the nominal interval even if fetch/build/publish fails.

Change scheduling so the next due time depends on result:

- On success: next due is nominal product interval.
- On failure: retry sooner using a capped short backoff.
- Track consecutive failures in memory and expose the absolute failure facts in
  status JSON.
- Reset failure backoff after success.

Do not hide failures behind fallback data. Failed attempts should be visible in
status even if the last successful product is still being served.

### Independent FAA Cycle Calendar

The monitor must own or fetch an independent expected-cycle calendar. Do not
derive expected future cycles only from publisher output.

Reason: if the publisher stops discovering or publishing cycle `2607`, a
published artifact cannot tell us that `2607` should already exist.

Acceptable first implementation:

- Check in a static FAA cycle calendar file and update it yearly.
- The monitor reads it and computes expected products for each cycle.

Better later implementation:

- Fetch a canonical FAA cycle schedule independently.

Monitor computes the due date from this independent source, for example:

- `due_at_utc = effective_date - 20 days`
- warning if expected product is more than 1 day late
- critical if expected product is more than 3 days late

### Monitor Daemon

Add a pipeline-health daemon that periodically gathers facts and writes both raw
history and evaluated current health.

Suggested inputs:

- `/health.json`
- `/build-watch/api/state`
- `/packages/current_artifacts.json`
- each referenced publication's `product-facts.json`
- `/live-feeds/status.json`
- monitor-owned FAA cycle calendar

Suggested outputs:

- `/pipeline-health/current.json`: most recent evaluated health.
- `/pipeline-health/history.json?limit=N`: recent raw/evaluated samples.
- `/pipeline-health/status.html`: presentation-only dashboard.

Disk backing:

- Append raw fact/evaluation samples to JSONL under the data root, for example
  `/mnt/aerobag-data/health/pipeline-health.jsonl`.
- Atomically rewrite current evaluated status, for example
  `/mnt/aerobag-data/health/pipeline-health-current.json`.

Run cadence:

- Poll every 1 minute initially.
- The daemon should tolerate missing inputs and stale inputs and report those as
  facts/evaluated metrics.
- The alerting system is out of scope; it should be able to watch only
  `/pipeline-health/current.json` for top-line state or for absence/staleness of
  that file.

### Metric And Evaluation Model

Raw samples are facts. Evaluated metrics are derived scalar records with
thresholds. Example evaluated metric:

```json
{
  "id": "live_feed.metars.stale_seconds",
  "label": "METAR stale age",
  "value": 245,
  "unit": "seconds",
  "severity": "ok",
  "warning_threshold": 300,
  "critical_threshold": 1800,
  "comparison": "greater_than",
  "source": "live-feeds/status.json"
}
```

Top-line status is the max severity over all evaluated metrics:

`ok < warning < critical`

Initial live-feed stale thresholds:

- TAF: warning `>1h`, critical `>3h`
- METAR: warning `>5m`, critical `>30m`
- obstacles: warning `>2d`, critical `>7d`
- TFR: warning `>3h`, critical `>6h`
- NEXRAD: warning `>5m`, critical `>15m`

For now, assume live-feed products should change each time according to their
product cadence. If a product later proves legitimately unchanged for long
periods, split metrics into `last_success_age_seconds` and
`last_changed_age_seconds` and adjust thresholds explicitly.

Initial cycle metrics:

- Latest build failed.
- Latest successful current-artifacts age.
- Expected cycle missing by due date.
- Per-product published/fetched/source timestamps.
- Per-product error count.
- Per-product warning count.
- Per-product error count increased from previous cycle.
- Per-product warning count increased from previous cycle.

Initial live-feed metrics:

- Per-product stale seconds.
- Per-product consecutive failure count.
- Per-product failure rate over monitor history windows, such as 1h and 24h.
- Per-product current error count.
- Per-product current warning count.
- Error count increased from previous version.
- Warning count increased from previous version.
- NEXRAD palette quality scalar metrics.

### Dashboard

The HTML dashboard is presentation only. It should fetch monitor JSON and render:

- Top-line status: ok/warning/critical.
- Current alert list, grouped by severity.
- Scalar metric table.
- Plotly line graphs for stale age and failure rate over time.
- Product-specific sections for cycle products and live-feed products.
- Links back to source surfaces such as `/build-watch/`,
  `/live-feeds/status.html`, `current_artifacts.json`, and detailed diagnostics.

Do not put threshold computation in the browser.

## Suggested Implementation Order

1. Extend live-feed daemon status facts.
   - Record attempts, successes, unchanged successes, failures, phases, errors,
     and nominal intervals.
   - Preserve recent attempt samples in `/live-feeds/status.json`.
   - Add tests proving failed fetch/build/publish attempts are visible in status.

2. Add fast retry for live-feed failures.
   - Replace simple "last polled at" interval behavior with result-aware
     scheduling or an equivalent wrapper around the source/builder task.
   - Add tests proving a failed attempt is due again sooner than the nominal
     interval and resets after success.

3. Promote cycle publisher facts.
   - Add `product-facts.json` or equivalent scalar fact payload to each
     publication.
   - Include error/warning counts by product and by diagnostic family.
   - Promote procedure-geometry and vector validator counts into those facts.

4. Add monitor-owned cycle calendar.
   - Start with a checked-in static FAA cycle schedule.
   - Use it to detect missing future cycle products even when publisher output
     stops advancing.

5. Implement pipeline-health daemon.
   - Fetch the status/fact sources.
   - Append raw samples to JSONL.
   - Compute evaluated metrics, thresholds, and top-line severity.
   - Serve `current.json`, bounded history, and status HTML.

6. Wire deployment.
   - Add systemd service/timer or long-running service as appropriate.
   - Expose under nginx as `/pipeline-health/`.
   - Include service state in `/health.json`.

7. Add dashboard.
   - HTML/Plotly presentation over monitor JSON only.
   - No client-side health computation beyond display formatting.

<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cycle publishers emit compact absolute product facts, including fetched/published/source timestamps and scalar error/warning counts.
- [ ] #2 Procedure-geometry and vector/HAD validator warning/error counts are promoted into cycle product facts.
- [ ] #3 Live-feed status exposes attempts, successes, unchanged successes, failures, phases, errors, nominal intervals, and current version facts per product.
- [ ] #4 Live-feed network/build/publish failures trigger a short faster retry sequence and are still visible in status.
- [ ] #5 NEXRAD palettization quality facts are exposed for monitor consumption.
- [ ] #6 Monitor owns an independent FAA cycle calendar and can alert when an expected future cycle is missing.
- [ ] #7 Pipeline-health daemon writes append-only raw fact history and a current evaluated health JSON document.
- [ ] #8 Evaluated health contains scalar metrics, warning/critical thresholds, and a top-line max severity.
- [ ] #9 Dashboard renders monitor JSON with Plotly graphs and alert summaries without doing health computation in browser.
- [ ] #10 Deployment exposes the machine and HTML health surfaces under production nginx.
<!-- AC:END -->
