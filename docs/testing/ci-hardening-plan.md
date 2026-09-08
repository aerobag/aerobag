# CI hardening: fast releases and reproducible failures

## Release path (implemented)

- Normal operation is `tools/prod_manage.py --stage`: a cached, exact-commit,
  emulator-free ordinary-CI preflight, then staging deployment and hosted
  exact-tag qualification concurrently. Do not add full local qualification.
- `--prequalify` is optional: every local lane once, no hosted candidate push or
  wait. Repeated stability tests have separate receipts and run titles.
- The warm fast preflight measured 2–3 minutes; this is not a cold-build timeout.
  Full local qualification is estimated at 18 minutes versus a measured 22m47s
  hosted single-pass run. Neither estimate is a reason to remove coverage.
- Promotion still requires deployed checks and exact-release hosted results.
  Do not stage/promote as part of this hardening work.

## Evidence and distinctions

Observed failures include browser-context reset/module loading, assertions on
uncomposed Android rows, compilation inside a server-readiness timeout, historical
NAVDB fixture availability, and a hosted Chrome/CDP startup stall. The first
four motivated concrete isolation, observation, lifecycle, and fixture fixes.
Not every CI failure is intermittent: missing `find_route` surface coverage on
the new airway-routing revision is a deterministic coverage failure, not a flake.

Hosted Chrome failure evidence:

- [Candidate run](https://github.com/aerobag/aerobag/actions/runs/34255587822)
- [Release first attempt](https://github.com/aerobag/aerobag/actions/runs/34264045527/attempts/1)
- `chrome.connect`: `Browser.getVersion` timed out before app navigation.
- Chrome was alive; teardown required SIGKILL. D-Bus messages were present but
  are not established as the cause. A later manual rerun passed.

## Implementation sequence and acceptance

### 1. Chase down the unexplained hosted Chrome startup stall

- [x] Add a small, bounded, manually dispatchable hosted diagnostic using the
  actual shared Chrome launcher/CDP transport; no app compilation or emulators.
- [x] Record browser and runner identity, per-phase elapsed time, process/thread
  state, resource pressure, bounded stderr, and CDP transport progress on failure.
  Do not capture credentials or arbitrary environment dumps.
- [ ] Compare controlled first-launch/repeated-launch conditions and pipe versus
  websocket transport with the same browser. Use evidence to distinguish slow
  scheduling, browser initialization, environment, and our protocol code.
- [ ] Pin the testing browser for local/hosted reproduction; keep upgrades
  explicit. Assess replacing browser lifecycle/transport with a maintained
  library through a small comparison, not a rewrite of shared journey semantics.
- [ ] Fix demonstrated defects and verify in the environment that failed.
  A non-reproduction is not a root-cause explanation. Record unresolved evidence.

If a narrowly classified pre-application startup restart is necessary, it must
have a fixed budget, fresh process/profile, preserved first-attempt evidence,
and a visible infrastructure-recovery result. Do not retry application actions
or whole journeys into green. Diagnostics/stability runs cannot qualify releases.

### 2. Audit causal completion and deadline ownership

- [ ] Audit the shared transition/observation helpers and their callers for
  preexisting/stale completion, unrelated revision changes, swallowed terminal
  errors, hung probes/actions, and unbounded nested waits.
- [ ] Keep mutation delivery exactly once. Await operation-specific outcomes
  and rendered postconditions; a few unchanged samples do not prove quiescence.
- [ ] Separate build, process readiness, resource completion, and functional
  deadlines. Retain product timing assertions; never hide a real user race by
  waiting for a private state the user cannot observe.
- [ ] Add executable adversarial regressions for demonstrated gaps, including
  terminal/transient errors and delayed/reordered observations.

### 3. Audit and test reset/isolation

- [ ] Inventory browser contexts/storage/workers, fixture-server mutations,
  cloud accounts, Android baseline data, ports/processes, and clock ownership.
- [ ] Verify fresh reset versus storage-preserving reload, and that failed
  teardown cannot silently contaminate the next test.
- [ ] Test a journey alone and after a deliberately dirty predecessor. Test
  delayed work crossing a reset boundary; preserve fresh AVDs per Android shard.

### 4. Control time and scheduling where they are not the subject

- [ ] Audit the fixture clock's boundaries: the current web clock fixes the
  epoch but advances with real elapsed time and is not a deterministic scheduler.
- [ ] Add controllable scheduling to focused harness/core tests as appropriate;
  deliberately exercise response-after-reset and out-of-order completion.
- [ ] Keep genuine browser rendering/network/clock integration in representative
  journeys. Do not replace visible behavior with direct state injection.

### 5. Validate the harness, not merely its source spelling

- [ ] Add behavioral tests for browser startup/exit, lost notifications, control
  replacement, teardown failure, and slow or terminal operations as indicated by
  the audits. Static contract tests alone cannot prove asynchronous correctness.
- [ ] Run cheap relevant suites for each commit and focused real-browser/device
  checks for changed boundaries; reserve full matrix runs for integrated proof.
- [ ] Preserve first-attempt outcomes and list bounded recovery separately.
  Stress chosen schedules/conditions, not just five identical warm repetitions.

## Execution rules

Implement and commit in reviewable chunks. Keep this document updated with
findings, exact validation, and remaining items. Do not declare the whole plan
complete because a rerun passed. Do not add a new release-path gate, expand
production state, weaken assertions, or erase failure artifacts.

## Work log

- 2026-09-08: Confirmed release-path policy already implemented in `ea5963f0`.
  Production was promoted separately by the operator; hardening does not change
  that release. Wrote this plan before implementation.
- 2026-09-08: Added an isolated eight-cell hosted startup diagnostic: installed
  versus checksum-pinned Chrome 152.0.7977.82, pipe versus websocket, inherited
  versus absent session-bus environment. Each cell records 100 independent
  fresh-profile launches, bounded to six minutes, with first failures retained.
  This workflow cannot qualify a release. Local smoke: three starts per
  transport passed; installer/release-tool Python tests: 102 passed; actionlint
  1.7.7 and whitespace checks passed. Existing foundation suite: 273/274;
  `find_route` surface coverage remains the previously reported failure.
