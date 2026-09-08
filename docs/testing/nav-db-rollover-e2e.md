# NAVDB Rollover Browser E2E

`ui/web-app/scripts/nav-db-rollover-e2e.mjs` proves that a running web client
handles an effective-cycle transition without replacing the user session.

The test builds NAVDBs from the permanent, small logical source in
[`crates/nav-db-fixture`](../../crates/nav-db-fixture/README.md), using the
production root/page encoder and Stored-ZIP/XZ writer. It does not fetch FAA
archives, require historical publications, mock NAVDB reads, or invoke session
mutation APIs directly. The generator rejects an obsolete source contract.

## Scenarios

- `success`: starts on synthetic generation 9901, constructs `KRNT SEA KPAE` plus
  `KPAE VOR-A ECEPO` through the visible flight-plan controls, crosses the 9902
  effective instant, and verifies that core adopts 9902 exactly once while
  preserving the plan. KRNT's displayed airport name must change to the
  candidate value, proving that the new database contents are actually in use.
- `reject`: builds a structurally valid 9902 NAVDB without
  `navref/position/navaid/SEA`, constructs the same plan, crosses the effective
  instant, and verifies that core keeps 9901, preserves the plan and original
  airport-info value, raises
  `nav_db:advance` with the reload action, and blocks repeated adoption. It
  then returns to the chart page, clicks the real `/!\` launcher, and verifies
  that the visible warning tray presents the failure and enabled reload action.

The preprocessor-side `nav_db_rollover_lab` binary generates the minimal
publication tree and records the logical source hash in `lab.json`. The browser
discovers that tree through `current_artifacts.json`, bundle manifests, and
unpacked NAVKV resources. The lab creates controlled validity windows;
`navDbMaintainAt` drives maintenance across that boundary without waiting for a
real date change. No FAA cycle turnover requires refreshing the source.

## Run

```sh
cd ui/web-app
npm run e2e:nav-db-rollover
```

Useful options pass through after `--`:

```sh
npm run e2e:nav-db-rollover -- --scenario success
npm run e2e:nav-db-rollover -- --scenario reject --headed
npm run e2e:nav-db-rollover -- --no-record
```

The default output is
`/tmp/aerobag-nav-db-rollover-e2e/<run-id>/`. Each scenario records mechanical
assertions, source/scenario metadata, before/after screenshots, browser
diagnostics, and an animated GIF.
CI should use the process exit status and `assertions.json`; the visual
artifacts explain failures but do not determine pass/fail.

The web-only `window.__aerobagE2e.navDb()` probe is read-only. It exposes the
active NAVDB identity, nav-data epoch, next maintenance deadline, advance
warning, active plan identity, and stable fields from core-projected flight-plan
rows. It is not a second control path.
