# App Contract Handoff 2026-04-24

Context:
- The preprocessor public contract was cleaned up.
- App/runtime consumers should now treat `current_artifacts` as a thin discovery document.
- Install/planning should flow through bundle manifests.
- Runtime semantic lookup should flow through `nav_db`.

Primary docs:
- `docs/PACKAGED_PUBLICATION_CONTRACT.md`
- `docs/PACKAGE_MANIFEST_CLEANUP_PLAN.md`

Current published shape:
- `current_artifacts.json` and `current_artifacts_*.json`
  - JSON list of current version-specific publication manifests
- `version_artifacts_*.json`
  - single build/version publication manifest used as merger input
  - `schema_version`
  - `contracts`
  - `as_of_date`
  - `bundles`
  - `diagnostics`
- cycle bundle:
  - `bundle_type = "cycle"`
  - hashed filename
  - `packages[]`
  - no `ancillary`
- fast bundle:
  - `bundle_type = "fast"`
  - hashed filename
  - `packages[]`
- no published `resource_index`
- no published `catalog`
- no published `data/main.db`
- no `current_artifacts.static_products`
- no legacy `current_artifacts.fast_products`
- no `current_artifacts.obstacles`

What was updated on the app side in this pass:
- `snapshot_artifacts.py` was retired.
  - dev clients should point at the artifact root directly.
  - `preprocessor-cli merge-current-artifacts` publishes the list-form `current_artifacts.json`.
- `ui/web-app/vite.config.ts`
  - validates the list-form `current_artifacts.json`
  - exposes the artifact root directly at `/packages`
  - removed dead packaged aliases for `catalog` / `resource_index`
- focused web test fixtures no longer depend on packaged `catalog` / `resource_index` data

What was actually validated:
- `npm test` in `ui/web-app`
- `npm run build` in `ui/web-app`

Result:
- direct artifact-root validation passes
- web tests pass
- web production build passes

Known remaining risk:
- `ui/core-rust` and `app-ffi` still contain some legacy `resource_index`-named APIs and chart-page derivation paths.
- I did not redesign those blind.
- If runtime behavior breaks even though build/test is green, that is the first place to inspect.

Recommended next step:
1. Launch one real UI target.
2. Exercise startup, chart loading, vectors, terrain, and live-feed loading against the new artifacts.
3. If anything fails, remove or adapt the remaining legacy `resource_index` assumptions in `ui/core-rust` / `app-ffi`.
