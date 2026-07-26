# Web/Android Action Parity

The parity runner should exercise action classes, not incidental labels. Web is
allowed to be the first implementation surface, but every reachable action class
must either be reachable on Android or appear as an explicit parity gap.

Current runner:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5554
```

The flight-plan WX modal regression has a focused red/green path:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both \
  --focus plan-weather \
  --url http://127.0.0.1:8082/ \
  --serial emulator-5554
```

The runner now records structured inventories in addition to boolean checks.
Inventories compare stable action IDs plus enabled/disabled, selected, active,
and toggle state. Labels are captured for diagnostics and compared when both
platforms expose them. This is intended to catch the common parity failure mode
where both platforms have "a tray", but the rows, disabled states, or subactions
quietly diverge.

Covered action classes:

- Page navigation: CDI to PLAN, CDI back to the most recent chart/plate surface,
  HOME, CHART/PLATE.
- Chart viewport: drag/pan and chart search recenter.
- Chart trays: map family selector and layer toggles for vectors, observations,
  NEXRAD, and terrain warning.
- Chart inspection: open inspect tray, select an item, and invoke Insert in
  flight plan.
- Plate page: airport selector, chart selector, load-procedure launcher, and
  folder launcher.
- Flight plan entry: free-form route feedback and append commit.
- Flight plan global controls: Next Leg, Sequence, Suspend, Unsuspend.
- Flight plan row actions: row action tray plus core row actions such as
  Activate Leg, Direct-To, Insert Before/After, Move Up, and Move Down.
- Flight plan weather: one tap on an enabled airport-row WX action must replace
  the row tray with the weather detail modal, without a second input event.

Current inventories:

- `chart.layers`: Vectors, METARs, NEXRAD, Terrain Warning, including toggle
  on/off and disabled state.
- `chart.map-family`: all reachable chart source options.
- `chart.inspect.items`: items found by the map inspector at the tested point.
- `chart.inspect.selected-actions`: all actions available for the selected
  inspected item.
- `plate.controls`: top-level plate page controls and enabled state.
- `plate.airports`, `plate.charts`, `plate.loads`: every option inside those
  trays.
- `plan.row.first.actions`, `plan.row.last.actions`: all row actions and their
  disabled state at the beginning and end of a plan.

When adding a new reachable web action class, add a parity tag/test id and add
the matching Android journey assertion in the same change. A missing Android
implementation should be recorded as a journey gap instead of silently falling
out of coverage.

For this to stay useful, every new tray row and action button needs a stable
core-derived ID in the web `data-testid` and Android `parity:` tag. If a control
cannot be identified by a stable ID, it is not parity-testable yet and should be
treated as test debt, not as covered UI.
