# Aerobag Agent Rules

- Keep shared behavior in `ui/core-rust`. Platform UI layers are view/controllers: they render core-exported models and dispatch core commands.
- Do not invent one-off platform widgets when an existing UI mechanism fits. Reuse the established tray/button machinery for tray-opening controls on web and Android.
- If a feature must behave the same across web and Android, model the state, choices, labels, selection, and side effects in core first. Platform code should not duplicate that logic.
- Before changing or diagnosing hosted tests, read [`docs/testing/hosted-ci.md`](docs/testing/hosted-ci.md). Keep CI inputs hermetic, test selection exact, and external readiness waits bounded and diagnostic.
- Run the fast release preflight before staging. Full local prequalification is an optional deeper gate, not a prerequisite for discovering ordinary build and unit-test failures.
- Treat published contract identifiers as immutable. A key, encoding, or required-shape change needs a new descriptor/version and matching fixture metadata; do not weaken readers with compatibility fallbacks.
- When adding or changing monitored product telemetry, update the producer contract and rule together; see `contracts/telemetry/README.md`. Never infer unsupported telemetry from a missing field, extend the frozen legacy registry, or invent approval for a monitoring-coverage removal.
- Generate platform wire enums and invalidation names from the owning core contract. Do not hand-copy them into Kotlin, TypeScript, fixture locks, or E2E prefix allowlists.
- Use the shared Android indexed-control modifiers for core-driven controls. They own both the Compose test tag and E2E geometry/state registration.
- Read NAVKV manifests, roots, and pages through the shared directory reader so compression, paths, and errors have one implementation.
- E2E map journeys must ask the semantic driver for an unobscured point derived from rendered geometry. Do not encode fractional or absolute map tap coordinates in journeys.
- Bind new geographic overlays to the shared map frame: web `MapGeometryBinding`/`MapGeometryLayer` uses the immediate content transform; Android uses `MapDisplayFrame` from the map display frame. Keep screen controls outside geographic transforms, and use the live frame for pointer conversion. Gate map actions with core’s `map_interaction` policy.

## When asked to commit and push

Run `/usr/bin/python3 tools/ci/cheap_preflight.py` before committing. It runs
all inexpensive suites on the current working tree; do not substitute a
hand-picked test list. A core enum can break a JavaScript journey contract,
even when no JavaScript file changed.

The command includes all fixture-free Rust tests/doctests, Python tool tests,
web unit/type checks, Android JVM/static tests, browser E2E harness contracts,
workflow lint, licensing, Rust formatting, generated UI sources, fixture-contract
metadata, and diff checks. It shares ordinary-CI suite definitions with release
preflight. `--list` shows the exact commands. Browser/emulator journeys, external
fixture replays, package production, and full web/native app builds remain
separate checks when the change warrants them.

- Aim for about two minutes on warm caches. Cold builds/dependency setup can take
  longer; suite deadlines fail explicitly and preserve logs. Report unexpected
  cost or missing prerequisites before extending the deadline.
- Inspect the final diff and stage only intended changes; preserve other
  sessions' work. Check after the final edit or integration change.
- Keep generated sources current. The preflight checks them in temporary paths
  before any build can regenerate them; fix stale output and inspect its diff.
- Fix in-scope failures without weakening assertions or retrying into green.
  Do not push known failing relevant checks without the user's explicit acceptance.
- Report exact checks and results, and name unrun checks. Cheap preflight passing
  does not mean complete ordinary CI, fixture CI, or release journeys passed.

When refactoring a helper, update structural tests to verify the new helper and
its implementation preserve the contract. Do not merely remove the assertion
or rename production code to satisfy a stale text match.

Before pushing a client contract bump, publish genuinely rebuilt compact
fixtures and update their pinned artifact-repository commit and contract
metadata together. Coordinate this handoff with the producer owner; a client
requiring NAV25 while the lock still supplies NAV24 is immediately CI-broken,
even when that feature's Rust tests pass. Never relabel old fixture bytes to
make the metadata check pass.

NAVDB rollover is different: its permanent logical input lives in
`crates/nav-db-fixture/source.json` and generates both scenarios locally. Migrate
those logical records and their descriptor when the contract changes; never
restore a dependency on two historical FAA cycles. Smoke/release fixtures still
come from one real available publication.

## Before staging a release

- The command is `tools/prod_manage.py --stage`, not `--staging`. Check ordinary
  CI for the integrated revision, not only an individual feature's local tests.
  Recommend resolving known failures before spending another staging build;
  report failed, pending, or unrun checks distinctly.
- Prefer `tools/prod_manage.py --stage` directly for routine releases: fast local
  checks, then deployment and one complete hosted exact-tag qualification in
  parallel. Do not add a full local or hosted candidate run by default.
- `tools/prod_manage.py --prequalify` is optional deeper local assurance. It
  requires clean, synchronized `main`, runs ordinary CI and every release journey
  once, and stops locally: no GitHub candidate push, hosted wait, or deployment.
  Its fast-check receipt is reused by a later `--stage` of that exact commit.
  This remains an explicit release operation, not a cheap commit-and-push check.
- Repeated flake/stability testing is separate from release qualification. Use
  an explicit `tools/ci/local_candidate_qualification.py --repetitions N` or
  hosted manual repetition input; never silently add repetitions to a release.
  Stability receipts/runs do not substitute for routine qualification.
- Run `tools/ci/fast_release_preflight.py` for the complete emulator-free
  ordinary-CI preflight on a clean integrated commit. `--stage` now runs it
  automatically before creating release intent or a tag. This is broader than
  the cheap working-tree preflight and is cached against the exact commit.
- Full prequalification remains optional. The operator may choose `--stage`
  directly and use the release-tag journey qualification round trip. Do not
  treat the full local qualifier's `--check` receipt verification as running tests.
- After staging, use `tools/prod_manage.py --qualification-status`: passing
  deployed staging checks alone does not mean ordinary CI and exact-release
  journeys passed. Do not automatically bypass failures with `--promote --force`.
