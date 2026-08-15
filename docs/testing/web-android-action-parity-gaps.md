# Web/Android Action Parity Gaps

Current inventory run:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5558
```

The journey itself completes on both platforms. It now drives both surfaces
through a mobile-width chart viewport, searches for `KBFI`, opens the chart
inspection tray at the recentered map location, selects `airport-KBFI`, and
executes `Insert`.

Current remaining divergence:

- `chart.inspect.items`: both platforms select `airport-KBFI`, expose the same
  selected-airport action set, and enable `TAF`. The full tray inventory still
  differs because the web click and Android tap do not hit exactly the same
  logical point/footprint. Web sees `airport-9WA0`, `airport-KBFI`,
  `weather-KBFI`, `navaid-SPOT`, nearby fixes, and the overlapping Seattle
  airspaces. Android sees those common items plus extra nearby airports
  (`01WT`, `W36`, `WN93`) and `navaid-DODVE`, while it does not surface the
  synthetic spot item in the UIAutomator-visible inventory. Next step: compare
  a core-supplied inspect inventory for a shared lat/lon and touch radius, or
  teach both drivers to use the same core-generated test hook instead of
  platform-specific screen taps.

Resolved in this pass:

- `chart.inspect.selected-actions`: Android now ingests TAFs through the native
  session bridge, and web fetches TAFs before committing the METAR ingest, so
  partial weather ingestion no longer leaves the selected airport with a
  disabled `TAF` action.
- `chart.layers.metars`: Android augments its vector manifest with the dynamic
  METAR layer metadata from the dev server, so the Observations layer is
  available in the same journey as web.
- `chart.inspect.target`: the journey now uses `KBFI` as an exact selected item
  target instead of selecting the first visible airport from each platform's
  tray inventory.
- `chart.inspect.selected-actions`: Android now renders core-supplied disabled
  placeholder actions, including `runways` and disabled `taf` when appropriate.
- `plate.airports`: the web journey now waits for the core route-entry ready
  state before submitting `KBFI`, so both platforms include the appended airport.
- `plate.controls`: Android now reports `LOAD APPCH` disabled when there are no
  procedure load options, matching web.
- `chart.inspect.insert`: the journey now appends `KAWO`, then selects `KBFI`
  from chart inspection, so the selected airport is off-plan and both platforms
  expose and execute `Insert`.
