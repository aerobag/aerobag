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
- `current_artifacts_*.json`
  - `schema_version`
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
- no `current_artifacts.fast_products`
- no `current_artifacts.obstacles`

What was updated on the app side in this pass:
- `snapshot_artifacts.py`
  - snapshots the new contract shape
  - copies `current_artifacts`, referenced bundles, referenced package files, and diagnostics
- `ui/scripts/stage_dev_assets.py`
  - stages from `current_artifacts.bundles[]`
  - resolves cycle and fast assets from bundle `packages[]`
  - stages `nav_db` from the `nav-db` package
  - no longer expects legacy top-level `resource_index`, `catalog`, `nav_kv`, `fast_products`, `static_products`, or `obstacles`
- `ui/web-app/vite.config.ts`
  - removed dead packaged aliases for `catalog` / `resource_index`
- web sample fixtures
  - replaced dead imports of packaged `catalog` / `resource_index` with local fixtures so tests still run

What was actually validated:
- `python3 snapshot_artifacts.py`
- `python3 ui/scripts/stage_dev_assets.py`
- `python3 -m py_compile snapshot_artifacts.py ui/scripts/stage_dev_assets.py`
- `npm test` in `ui/web-app`
- `npm run build` in `ui/web-app`

Result:
- web staging passes
- web tests pass
- web production build passes

Known remaining risk:
- `ui/core-rust` and `app-ffi` still contain some legacy `resource_index`-named APIs and chart-page derivation paths.
- I did not redesign those blind.
- If runtime behavior breaks even though build/test is green, that is the first place to inspect.

Recommended next step:
1. Launch one real UI target.
2. Exercise startup, chart loading, vectors, terrain, and fast-product loading against the new artifacts.
3. If anything fails, remove or adapt the remaining legacy `resource_index` assumptions in `ui/core-rust` / `app-ffi`.
