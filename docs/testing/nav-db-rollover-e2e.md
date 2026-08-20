# NAVDB Rollover Browser E2E

`ui/web-app/scripts/nav-db-rollover-e2e.mjs` proves that a running web client
handles an effective-cycle transition without replacing the user session.

The test uses immutable production NAVDB fixtures matching the current client
contract from `aerobag-test-artifacts.git` under
`nav-db/advance-2608-to-2609`. It does not mock NAVDB reads or invoke session
mutation APIs directly. CI verifies the fixture contract before installing
toolchains or starting a browser.

## Scenarios

- `success`: starts on 2608, constructs `KRNT SEA KPAE` plus
  `KPAE VOR-A ECEPO` through the visible flight-plan controls, crosses the 2609
  effective instant, and verifies that core adopts 2609 exactly once while
  preserving the plan.
- `reject`: rebuilds a structurally valid 2609 HAD without
  `navref/position/navaid/SEA`, constructs the same plan, crosses the effective
  instant, and verifies that core keeps 2608, preserves the plan, raises
  `nav_db:advance` with the reload action, and blocks repeated adoption. It
  then returns to the chart page, clicks the real `/!\` launcher, and verifies
  that the visible warning tray presents the failure and enabled reload action.

The preprocessor-side `nav_db_rollover_lab` binary verifies the source fixture
hashes and generates the minimal publication tree. The browser still discovers
that tree through `current_artifacts.json`, bundle manifests, and unpacked HAD
resources.

## Run

```sh
cd ui/web-app
npm run e2e:nav-db-rollover
```

Useful options pass through after `--`:

```sh
npm run e2e:nav-db-rollover -- --scenario success --transition-seconds 30
npm run e2e:nav-db-rollover -- --scenario reject --headed
npm run e2e:nav-db-rollover -- --no-record
```

The default output is
`/tmp/aerobag-nav-db-rollover-e2e/<run-id>/`. Each scenario records mechanical
assertions, before/after screenshots, browser diagnostics, and an animated GIF.
CI should use the process exit status and `assertions.json`; the visual
artifacts explain failures but do not determine pass/fail.

The web-only `window.__aerobagE2e.navDb()` probe is read-only. It exposes the
active NAVDB identity, nav-data epoch, next maintenance deadline, advance
warning, active plan identity, and stable fields from core-projected flight-plan
rows. It is not a second control path.
