# Aerobag Agent Rules

- Keep shared behavior in `ui/core-rust`. Platform UI layers are view/controllers: they render core-exported models and dispatch core commands.
- Do not invent one-off platform widgets when an existing UI mechanism fits. Reuse the established tray/button machinery for tray-opening controls on web and Android.
- If a feature must behave the same across web and Android, model the state, choices, labels, selection, and side effects in core first. Platform code should not duplicate that logic.
- Before changing or diagnosing hosted tests, read [`docs/testing/hosted-ci.md`](docs/testing/hosted-ci.md). Keep CI inputs hermetic, test selection exact, and external readiness waits bounded and diagnostic.
- Run the fast release preflight before staging. Full local prequalification is an optional deeper gate, not a prerequisite for discovering ordinary build and unit-test failures.
- Treat published contract identifiers as immutable. A key, encoding, or required-shape change needs a new descriptor/version and matching fixture metadata; do not weaken readers with compatibility fallbacks.
- Generate platform wire enums and invalidation names from the owning core contract. Do not hand-copy them into Kotlin, TypeScript, fixture locks, or E2E prefix allowlists.
- Use the shared Android indexed-control modifiers for core-driven controls. They own both the Compose test tag and E2E geometry/state registration.
- Read NAVKV manifests, roots, and pages through the shared directory reader so compression, paths, and errors have one implementation.
- E2E map journeys must ask the semantic driver for an unobscured point derived from rendered geometry. Do not encode fractional or absolute map tap coordinates in journeys.
- Bind new geographic overlays to the shared map frame: web `MapGeometryBinding`/`MapGeometryLayer` uses the immediate content transform; Android uses `MapDisplayFrame` from the map display frame. Keep screen controls outside geographic transforms, and use the live frame for pointer conversion. Gate map actions with core’s `map_interaction` policy.

## When asked to commit and push

Run a cheap feature preflight before committing, not just the new tests written
for the change. Target **under two minutes total on a warm checkout**. Select
checks from [ordinary CI](.github/workflows/ci.yml), with its pinned tools and
environment; a cold build, dependency installation, emulator, or full release
qualification is not implicitly part of this budget.

- Inspect the final diff and run `git diff --check`. Stage only the intended
  changes; preserve other sessions' work.
- Run relevant existing regression and contract tests as well as new tests.
  Test ownership crosses language boundaries: changing Kotlin or TypeScript
  adapters can break tests housed in Rust.
- Run applicable formatting and generated-source checks. For Rust edits, use
  `./scripts/check-rust-format.sh`. If generation is needed, inspect its tracked
  diff and include intended outputs; do not silently leave stale generated code.
- Use explicit empty artifact directories for fixture-free checks. Use the web
  target-workspace entrypoint rather than source-tree `node_modules`; use
  `ANDROID_BUILD_NATIVE_LIBRARIES=false` for Android JVM/static-only checks.
- Recheck after the final edit or integration change. A pass on an earlier tree
  does not validate the tree being committed.
- Fix in-scope failures without weakening assertions or retrying into green.
  If a needed check exceeds the budget, is blocked, or exposes an unrelated
  failure, report it and ask how to proceed rather than silently skipping it or
  starting a long suite. Do not push known failing relevant checks without the
  user's explicit acceptance.
- In the handoff, list exact checks and results, and explicitly name checks not
  run. A targeted pass is not a claim that all ordinary CI passed.

Choose the smallest useful coverage for the feature:

| Changed surface | Cheap checks to consider |
| --- | --- |
| Rust behavior | Affected crate/test family under nextest's `ci` profile with `--locked`; relevant doctests |
| Core/web/Android session adapters or UI contracts | The complete Rust `ui_core_boundary` test binary, plus affected platform tests |
| Python tools | Relevant `test_*.py` files with CI's `/usr/bin/python3 -m pytest` |
| Product/client data contracts or fixture locks | `/usr/bin/python3 tools/ci/verify_locked_fixture_contracts.py` (sub-second metadata check), plus verification of any rebuilt fixture bytes |
| JavaScript E2E tooling | Relevant `node --test` harness contract files, not browser/emulator journeys |
| Web or Android feature | Focused existing web unit/type checks or Android JVM/static tests through repo entrypoints |
| Workflow changes | Pinned actionlint command from `.github/workflows/ci.yml` |
| Documentation only | Diff/whitespace checks and verification of referenced paths and commands; no app build |

For core/platform boundary changes, run this from the repository root:

```sh
(
  cd ui/core-rust
  AEROBAG_ARTIFACT_READ_PATH="$(mktemp -d)" \
    cargo +1.94.1 nextest run --locked --profile ci \
      -p app-core --test ui_core_boundary
)
```

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
  the two-minute feature checklist and is cached against the exact commit.
- Full prequalification remains optional. The operator may choose `--stage`
  directly and use the release-tag journey qualification round trip. Do not
  treat the full local qualifier's `--check` receipt verification as running tests.
- After staging, use `tools/prod_manage.py --qualification-status`: passing
  deployed staging checks alone does not mean ordinary CI and exact-release
  journeys passed. Do not automatically bypass failures with `--promote --force`.
