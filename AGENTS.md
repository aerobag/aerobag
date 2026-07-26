# Aerobag Agent Rules

- Keep shared behavior in `ui/core-rust`. Platform UI layers are view/controllers: they render core-exported models and dispatch core commands.
- Do not invent one-off platform widgets when an existing UI mechanism fits. Reuse the established tray/button machinery for tray-opening controls on web and Android.
- If a feature must behave the same across web and Android, model the state, choices, labels, selection, and side effects in core first. Platform code should not duplicate that logic.
- Before changing or diagnosing hosted tests, read [`docs/testing/hosted-ci.md`](docs/testing/hosted-ci.md). Keep CI inputs hermetic, test selection exact, and external readiness waits bounded and diagnostic.
