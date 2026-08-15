# UI Parity Journeys

The parity harness drives the same semantic journey against web and Android.
It reports platform gaps and cross-platform divergences as first-class output.
The harness uses stable view/controller identifiers only; it should not
duplicate business logic from core.

Detailed action coverage lives in
[`docs/testing/web-android-action-parity.md`](testing/web-android-action-parity.md).
Known remaining limitations live in
[`docs/testing/web-android-action-parity-gaps.md`](testing/web-android-action-parity-gaps.md).

## Flight Plan Inspect Insert

Runner:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs web --url http://127.0.0.1:8082/
node tools/parity/run-flight-plan-inspect-journey.mjs android --serial emulator-5554
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5554
```

For a fast first-tap flight-plan weather regression run:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both \
  --focus plan-weather \
  --url http://127.0.0.1:8082/ \
  --serial emulator-5554
```

Current shared journey:

1. Wait for the chart/map surface.
2. Use the CDI/nav element to open PLAN.
3. Use the CDI/nav element to return to the most recent chart/plate surface.
4. Use the CDI/nav element to return to PLAN.
5. Inventory global flight-plan controls and first/last row actions.
6. Tap the destination airport's WX action once and require its weather modal
   to appear before any subsequent input.
7. Type an invalid free-form route and verify route feedback is visible.
8. Append `KAWO` through the free-form route-entry field.
9. Return to CHART and inventory chart layer and map-family trays.
10. Open PLATE and inventory airport, chart, and load-procedure controls/trays.
11. Return to CHART, drag the map, search/recenter on `KBFI`, and open the chart inspector.
12. Select `airport-KBFI`, inventory its action tray, execute `Insert`, and verify `KBFI` appears in PLAN.

The `both` mode compares structured inventories and boolean checks between web
and Android. It currently gates stable action IDs, enabled/disabled state,
selected/active state, and layer toggle state. Labels are captured for
diagnostics and compared when both platforms expose them.

## Stable Identifiers

Web uses `data-testid`. Android uses accessibility content descriptions
prefixed with `parity:` so `adb shell uiautomator dump` can find the same
semantic controls.

Examples:

- `nav-cdi`
- `map-surface`
- `plan-append-route-input`
- `plan-row-action-<action-id>` on web and `parity:plan-row-action:<action-id>` on Android
- `map-selection-item-<category>-<label>` on web and `parity:map-selection-item:<category>-<label>` on Android
- `map-selection-action-<id>` on web and `parity:map-selection-action:<id>` on Android

Every new reachable tray row or action button should expose a stable
core-derived ID. If a control cannot be identified by a stable ID, treat that as
test debt rather than covered UI.
