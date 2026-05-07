# Web/Android Action Parity Gaps

Current inventory run:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5558
```

The journey itself completes on both platforms, but the structured inventory
comparison reports these divergences. Resolved items stay here briefly so we can
see what the current checkpoint fixed.

- `chart.inspect.items`: the platforms still need a deterministic shared inspect
  target. The current script uses the same KTIW search then taps the map center;
  depending on current flight-plan/map state that can select KBFI on web and a
  different nearby set on Android. This is a test-harness gap: the journey should
  drive both platforms to the same core inspect point, not infer it from pixels.
- `chart.inspect.insert`: after appending `KBFI`, the web center tap can select
  KBFI, which is already in the flight plan. Core correctly omits `Insert in
  flight plan` and exposes `Remove from flight plan` instead. The insert journey
  should select an off-plan airport, or split the plate-airport append coverage
  from the chart-inspector insert coverage.

Resolved in this pass:

- `chart.inspect.selected-actions`: Android now renders core-supplied disabled
  placeholder actions, including `runways` and disabled `taf` when appropriate.
- `plate.airports`: the web journey now waits for the core route-entry ready
  state before submitting `KBFI`, so both platforms include the appended airport.
- `plate.controls`: Android now reports `LOAD APPCH` disabled when there are no
  procedure load options, matching web.
