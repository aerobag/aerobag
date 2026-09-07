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
