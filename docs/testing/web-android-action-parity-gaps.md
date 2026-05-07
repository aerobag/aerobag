# Web/Android Action Parity Gaps

Current inventory run:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5558
```

The journey itself completes on both platforms, but the structured inventory
comparison reports these divergences. Resolved items stay here briefly so we can
see what the current checkpoint fixed.

- `chart.inspect.items`: the journey now drives web to the rendered `TIW` label
  and Android to the recentered KTIW map surface, so both platforms select the
  `airport-KTIW` item and expose the same selected-airport action set. The full
  tray inventory still diverges: web reports `weather-KTIW` and `navaid-SPOT`,
  while Android reports nearby navaids from its center tap and does not surface
  the weather/spot entries in the UIAutomator-visible inventory. Next step:
  make the Android journey target the same logical point or add a core-backed
  parity/debug inventory so the test compares core tray contents rather than
  only initially visible Android semantics nodes.

Resolved in this pass:

- `chart.inspect.selected-actions`: Android now renders core-supplied disabled
  placeholder actions, including `runways` and disabled `taf` when appropriate.
- `plate.airports`: the web journey now waits for the core route-entry ready
  state before submitting `KBFI`, so both platforms include the appended airport.
- `plate.controls`: Android now reports `LOAD APPCH` disabled when there are no
  procedure load options, matching web.
- `chart.inspect.insert`: the journey now appends `KAWO`, then selects `KTIW`
  from chart inspection, so the selected airport is off-plan and both platforms
  expose and execute `Insert in flight plan`.
