# Web/Android Action Parity Gaps

Current inventory run:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5558
```

The journey itself completes on both platforms, but the structured inventory
comparison reports these divergences.

- `chart.inspect.items`: web reports `airport-KTIW`, `airspace-SEATTLE CLASS B`,
  `airspace-TACOMA CLASS D`, `navaid-SPOT`, and `weather-KTIW`; Android reports
  `airport-KTIW`, the same two airspaces, and nearby navaids `ARVAD`, `NEECE`,
  `VPFOX`. The platforms are not inspecting the exact same chart point yet.
- `chart.inspect.selected-actions`: web exposes disabled `runways` and disabled
  `taf` placeholders for the selected airport; Android only exposes the enabled
  action buttons. Android should render the disabled core-supplied placeholder
  actions too.
- `plate.airports`: after the journey appends `KBFI`, Android includes it in the
  plate airport selector, while web only lists `KPAE` and `KRNT`. The platforms
  need the same core-owned recent/flight-plan-airport policy.
- `plate.controls`: web shows `LOAD APPCH` disabled for `KRNT RNAV 34`; Android
  shows `LOAD` enabled. The plate load button state/label must come from the same
  core procedure-load availability.

