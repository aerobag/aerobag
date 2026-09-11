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
the new airway-routing revision was a deterministic coverage failure, not a flake.
The real journey added in `72a11f50` then exposed two deterministic Android
product bugs in the feature. See the September 10 work log below.

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
- [x] Compare controlled first-launch/repeated-launch conditions and pipe versus
  websocket transport with the same browser. Use evidence to distinguish slow
  scheduling, browser initialization, environment, and our protocol code.
- [x] Pin the testing browser for local/hosted reproduction; keep upgrades
  explicit. Assess replacing browser lifecycle/transport with a maintained
  library through a small comparison, not a rewrite of shared journey semantics.
- [x] Fix demonstrated defects and verify in the environment that failed.
  A non-reproduction is not a root-cause explanation. Record unresolved evidence.

If a narrowly classified pre-application startup restart is necessary, it must
have a fixed budget, fresh process/profile, preserved first-attempt evidence,
and a visible infrastructure-recovery result. Do not retry application actions
or whole journeys into green. Diagnostics/stability runs cannot qualify releases.

### 2. Audit causal completion and deadline ownership

- [x] Audit the shared transition/observation helpers and their callers for
  preexisting/stale completion, unrelated revision changes, swallowed terminal
  errors, hung probes/actions, and unbounded nested waits.
- [x] Keep mutation delivery exactly once. Await operation-specific outcomes
  and rendered postconditions; a few unchanged samples do not prove quiescence.
- [x] Separate build, process readiness, resource completion, and functional
  deadlines. Retain product timing assertions; never hide a real user race by
  waiting for a private state the user cannot observe.
- [x] Add executable adversarial regressions for demonstrated gaps, including
  terminal/transient errors and delayed/reordered observations.

### 3. Audit and test reset/isolation

- [x] Inventory browser contexts/storage/workers, fixture-server mutations,
  cloud accounts, Android baseline data, ports/processes, and clock ownership.
- [x] Verify fresh reset versus storage-preserving reload, and that failed
  teardown cannot silently contaminate the next test.
- [x] Test a journey alone and after a deliberately dirty predecessor. Test
  delayed work crossing a reset boundary; preserve fresh AVDs per Android shard.

### 4. Control time and scheduling where they are not the subject

- [x] Audit the fixture clock's boundaries: the current web clock fixes the
  epoch but advances with real elapsed time and is not a deterministic scheduler.
- [x] Add controllable scheduling to focused harness/core tests as appropriate;
  deliberately exercise response-after-reset and out-of-order completion.
- [x] Keep genuine browser rendering/network/clock integration in representative
  journeys. Do not replace visible behavior with direct state injection.

### 5. Validate the harness, not merely its source spelling

- [x] Add behavioral tests for browser startup/exit, lost notifications, control
  replacement, teardown failure, and slow or terminal operations as indicated by
  the audits. Static contract tests alone cannot prove asynchronous correctness.
- [x] Run cheap relevant suites for each commit and focused real-browser/device
  checks for changed boundaries; reserve full matrix runs for integrated proof.
- [x] Preserve first-attempt outcomes and list bounded recovery separately.
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
- 2026-09-08: Hosted diagnostic [34272782000](https://github.com/aerobag/aerobag/actions/runs/34272782000)
  reproduced a pre-CDP first-start stall (installed Chrome/websocket): 15-second
  endpoint timeout, alive process, SIGKILL required. Samples repeatedly show
  `D (disk sleep)` / `folio_wait_bit_common`, with system I/O PSI `some avg10`
  reaching 60.63% and `full avg10` 59.86%; CPU/memory pressure remained low.
  Other installed-browser first launches took 6.9–9.0 seconds; subsequent
  launches were about 0.2–0.3 seconds. All 400 downloaded/pinned launches passed,
  with first launches 0.27–0.76 seconds. Runner image versions and installed
  Chrome patch versions varied across jobs: do not attribute this to a browser
  patch or D-Bus. The next controlled run preloads only installed executable/
  resource files to distinguish cold file I/O from browser/profile state.
  See [Linux PSI semantics](https://docs.kernel.org/accounting/psi.html).
- 2026-09-08: Controlled hosted [34273328729](https://github.com/aerobag/aerobag/actions/runs/34273328729)
  passed all 1,200 starts. Four cold installed-browser jobs first started in
  1.06–7.76 seconds; four preloaded jobs spent 2.50–16.11 seconds reading only
  executable/resources and then started in 0.49–1.61 seconds. Four pinned jobs
  first started in 0.76–1.96 seconds. This establishes cold runner-image file I/O
  as a reproducible startup bottleneck, including one captured timeout. The
  original 30-second pipe stalls lack process snapshots, so identical causation
  remains an inference, not a proven reconstruction of those runs.
- 2026-09-08: Pinned Chrome for Testing 152.0.7977.82 for local full qualification
  and hosted web, rollover, and Android desktop-cloud-peer jobs. No browser
  install in cheap preflight, no extra application retry, no raised functional
  deadline. Each cache belongs to the explicit run/workspace. The diagnostic
  now defaults to four pinned-browser jobs; `compare_system=true` restores the
  controlled comparison. Browser upgrades require changing version and SHA256.
- 2026-09-08: Small maintained-library comparison: Playwright Core 1.63.0,
  installed only in a disposable experiment workspace, launched the same pinned
  Chrome five times and verified page/context lifecycle in 129–149 ms. It is
  viable for a future lifecycle/transport replacement, but this warm local
  comparison does not prove it fixes hosted cold I/O. Do not rewrite semantic
  actions to adopt different implicit waits while diagnosing that issue.
- 2026-09-08: Added owned deadlines and cooperative cancellation for probes,
  event waits, action delivery, and failure diagnostics. Terminal precondition
  errors now stop before mutation; only explicitly transient reads may repeat.
  Added manually scheduled hung/late/error regressions without wall-clock sleeps.
  An action timeout ends the journey; cancellation cannot undo delivered input.
- 2026-09-08: CDP navigation now matches frame and loader identity, buffers early
  notifications, bounds missing events, and cancels on disconnect/replacement.
  Same-document navigation does not await a nonexistent load event. Endpoint
  failure owns process teardown; page disposal unregisters page/worker listeners;
  cloud-peer setup failure owns browser/profile cleanup. Reset destruction RPCs
  share their remaining budget instead of swallowing terminal protocol errors.
- 2026-09-08: Two fixture-server regressions failed before their fixes: reset did
  not rearm one-shot transport faults, and a control request whose body completed
  after reset could mutate the next test's publication. Reset now clears fault
  history and rejects old-generation control requests with HTTP 409.
- 2026-09-08: Local validation: 105 relevant Python tests passed; 328/329 complete
  E2E harness tests passed (only the known `find_route` coverage failure); two
  real-Chrome lifecycle tests passed. Focused retained-app diagnostics passed:
  `web.pointer-details` (1.5s), `shared.about-and-saved-state` (2.0s),
  `shared.cloud-crossfill` (11.7s). These use retained app `ee35e0cb`, not a rebuilt
  current application and not qualification receipts. Final hosted boundary
  validation is recorded below when available.
- 2026-09-08: Final committed-boundary verification at `c60a0b87`:
  [hosted run 34275418161](https://github.com/aerobag/aerobag/actions/runs/34275418161)
  passed 400/400 pinned Chrome starts (pipe and websocket on four fresh runners),
  with first starts 0.29–0.62 seconds. Both real-browser lifecycle tests passed
  on each runner, including delayed-worker isolation and failed-peer cleanup.
  Full local qualification was deliberately not run; no release was staged or
  promoted. Ordinary CI's separate coverage failure remains visible.
- 2026-09-10: Investigated release `2026-09-10.1` / `0ef1aa9c`,
  [hosted run 34533229388](https://github.com/aerobag/aerobag/actions/runs/34533229388).
  The sole failed execution lane was Android shard 3: `shared.flight-plan-find-route`
  timed out opening HOME after successfully displaying a route draft. The
  aggregate failure was derivative; every other execution/preparation job passed.
  The exact hosted APK, fixture, and baseline reproduce this alone on a fresh
  local emulator, excluding predecessor contamination. The rendered app was not
  black, and no Chrome startup failure was involved.
- 2026-09-10: Root cause 1, introduced by `63f3a803`: the route editor's
  full-screen high-z pointer handler won Compose sibling hit testing over HOME.
  Declining to consume an unrelated tap does not re-hit-test an underlying
  sibling. Introduced production-owned `MapSurfaceLayers`: geographic/editor
  layers and screen controls have separate parents, so local overlay zIndex
  cannot outrank screen controls. Three real Compose/Robolectric tests use
  physical input: a reproduction of the broken flat layout and portrait/landscape
  tests of the production container, including uncovered map input.
- 2026-09-10: Fixing HOME exposed root cause 2 in the same introducing commit:
  Flight Plan's page-local `remember(null)` replayed the existing draft as a new
  navigation request after HOME -> Flight Plan. The page immediately returned
  to the map. Added a production navigation effect that observes new editor
  activation, not mounting existing state, with two real Compose lifecycle tests
  including actual removal/reentry. No journey assertion, action, timeout, or
  retry policy was changed to make either fix pass.
- 2026-09-10: Why cheaper checks missed this: all 337 harness-model tests passed
  while their navigation stub directly changed a page variable, bypassing both
  physical hit testing and Android composition lifecycle. Added the five rendered
  component tests to the existing Android JVM suite (ordinary CI, cheap preflight,
  and fast release preflight); their focused runtime was about 2.2 seconds plus
  Gradle startup/compilation. Structural guards verify the actual pages use these
  tested boundaries. Updated agent/testing guidance to require physical-input and
  remount tests for these classes of changes and focused real-platform evidence
  for new journeys; a registry entry and passing mock tests are not that evidence.
- 2026-09-10: Earlier feedback was already available: main-branch
  [run 34437773361, shard 3](https://github.com/aerobag/aerobag/actions/runs/34437773361/job/102748480177)
  failed the same Find Route -> HOME action at 04:55 UTC, before the staging run.
  Android's main E2E already includes all priorities (web p1/p2 have narrower
  triggers). This is not an Android release-only selection gap. The preceding
  preflight-bootstrap push's E2E was canceled by the staging commit's main push;
  [that main run](https://github.com/aerobag/aerobag/actions/runs/34533229098/job/103062102182)
  also failed identically. Inspect and triage existing main E2E results separately
  from green ordinary CI, rather than starting another complete prequal or
  discovering the same red result again at release time.
- 2026-09-10: Local diagnostic validation, not a qualification receipt:
  19 focused JVM tests passed (including the five new rendered component tests).
  Rebuilt only Kotlin/UI, verifying its native library byte-for-byte against the
  hosted APK and retaining the hosted fixture/baseline. All eight Android shard-3
  journeys passed once, including real navigation, inspector details, weather,
  and saved-state restart; Find Route took 17.33 seconds, the journey span was
  188.82 seconds plus emulator/install setup. Retained evidence: original inputs
  `/tmp/aerobag-journey-investigation-U8VLa2`, original-APK isolated failure
  `/tmp/aerobag-journey-diagnostic-jtiuhdp4`, first-fix lifecycle failure
  `/tmp/aerobag-map-layer-fixed-vm12s1v5`, and final passing shard
  `/tmp/aerobag-map-lifecycle-fixed-hpz2zulg`. No new full hosted qualification,
  staging, promotion, or release-receipt claim was made.

- 2026-09-10: Release `2026-09-10.2` / `0c8391e5`,
  [hosted run 34539463288](https://github.com/aerobag/aerobag/actions/runs/34539463288),
  passed all Android shards but failed web p1's Cloud Device Setup Code copy
  confirmation. An ordinary isolated replay passed; controlled response ordering
  reproduced the same timeout: deliver an older successful cloud root read just
  after the real clipboard succeeds, and its action completion erases `Copied`
  18.6ms later. This is an application feedback-ownership race, not evidence
  that the journey needs a longer deadline. The hosted trace lacks completion
  ordering, so its precise historical interleaving remains unproved.
- 2026-09-10: CloudPage now gives only the latest invoked action permission to
  settle its feedback, for successes and errors. Six rendered React component
  tests control promise completion directly, including reverse order and stale
  errors; the initial four reproduced two failures in 47ms before the fix and
  passed in 48ms afterward. They run in ordinary web CI and cheap preflight,
  using pinned jsdom with no fixtures, browser startup, sleeps, or repetitions.
  No journey assertion or timeout was changed.
- 2026-09-10: One controlled real-Chrome crossfill replay passed after rebuilding
  the changed React UI (17.8s lane including setup). Diagnostic-only inputs:
  pinned Chrome 152.0.7977.82, the hosted cloud server and fixture, and cached
  local debug WASM from `b2ad0d9c` (unchanged core source at HEAD, not the hosted
  optimized bytes). Retained original failure:
  `/tmp/aerobag-journey-diagnostic-o5d87fhe`; rebuilt-UI pass and provenance:
  `/tmp/aerobag-cloud-feedback-fixed-9H6Jxo`. Both use the observation and
  response-order preloads under `/tmp/aerobag-cloud-journey-B1ArLy`. This is
  focused regression evidence, not a new full hosted qualification.

## Ownership and causal-completion audit

- 2026-09-11: Promotion of `2026-09-11.1` committed intent but failed activation
  when outgoing `2026-09-08.1` was retained in sunset. Both use NAV25 and identical
  package contract sets; the controller passed both to a merger that correctly
  rejects duplicate discovery contract sets. Traffic stayed on the old release.
  Fixed selection at the controller boundary (controlling release first), keeping
  all release-scoped endpoints and GC roots. New `promotion_merge` Cargo integration
  test reproduced the exact duplicate-contract error before the fix and passed
  afterward in 70ms, using the real controller and Rust executable. Existing mock
  controller tests had replaced the merger and could not detect this mismatch.
  The release manager now finishes with an explicit colored operation result
  after the log path; error, interrupt, pending and deployment-only states are
  tested without claiming full hosted qualification.

| Boundary | Owner and result |
| --- | --- |
| Browser storage, permissions, page and dedicated workers | Fresh context per reset; same context/new page per reload. Real test dirties cookies/localStorage/IndexedDB, holds a worker response across reset, and compares with an isolated clean run. Failed context disposal stops replacement. Page listeners are disposed with their page. |
| Fixture publication, fault injection, request evidence | Lab sends a single bounded control reset before each journey. Server reset now clears one-shot faults and increments an internal generation; delayed old controls cannot change new state. |
| Cloud accounts and peer | Lab uses disposable cloud storage/secret and starts a fresh server for crossfill. Peer setup now cleans up on failure. Crossfill intentionally uses real time because account/code expiry is part of the integration. |
| Android | Existing fresh AVD per shard, clean install, qualified baseline restoration and explicit reset remain unchanged. Do not share emulator state to gain parallelism. No device adapter behavior changed in this patch. |
| Ports/processes | Local qualification/diagnostics keep the host-wide lock and GUI phase isolation; hosted jobs have separate machines. Fixed-port legacy shell ownership remains a follow-up below. |
| Clock/scheduler | Web fixture clock is an epoch offset advancing with `performance.now`, not frozen time or controlled timers; workers are separate realms. Manual scheduler is used for harness deadline regressions, not injected into production rendering. FAA rollover continues using permanent logical fixtures. |
| Completion evidence | Session revision is diagnostic, not a universal completion predicate. Cloud action revision is incremented only by successful explicit UI actions (`cloud.rs`); cloud-active/rendered-state checks remain separate. Four stable-value callers establish geometry/scroll/cloud baselines, not global network quiescence. |

## Continuing hardening, not release prerequisites

- Inspect first-attempt results from the next full exact-tag qualification.
  This patch's targeted checks cannot establish that the entire suite is flake-free.
- The `find_route` manifest gap is closed by `72a11f50`; keep the real journey
  and the new fixture-free Android component regressions. Gradually replace
  source-only UI checks with production-owned input/lifecycle component tests
  where those boundaries are the subject, retaining end-to-end integration proof.
- Triage new main-branch E2E reds when they appear, including distinguishing a
  product defect from harness/infrastructure trouble. Cancellation or a green
  ordinary-CI result does not resolve an unchanged earlier product failure.
- Legacy smoke scripts still use older broad `waitFor` helpers; migrate them
  incrementally with behavioral tests. Shared release transition/reset boundaries
  were the implementation scope here, not every historical script.
- Replace legacy fixed-port/PID shell cleanup with explicit process ownership
  before allowing independent local labs to run concurrently. The existing lock
  remains mandatory; do not remove it based on an idle-machine benchmark.
- Revisit a maintained browser lifecycle library if protocol maintenance keeps
  generating defects; preserve atomic semantic actions and explicit product
  timing. Pinning and owned lifecycle fixes are not a claim that all Chrome
  startup failures share the captured I/O cause.
