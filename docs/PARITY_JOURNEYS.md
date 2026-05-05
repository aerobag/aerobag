# UI Parity Journeys

The parity harness drives the same user journey against web and Android and reports either a pass or explicit gaps. It is intentionally semantic: stable widget IDs/content descriptions identify controls, and screenshots can be layered in later for visual regression.

## Flight Plan Inspect Insert

Runner:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs web --url http://127.0.0.1:8082/
node tools/parity/run-flight-plan-inspect-journey.mjs android --serial emulator-5554
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5554
```

Web path:

1. Wait for chart/map surface.
2. Open the flight-plan page through the CDI/nav element.
3. Append `KBFI` through the free-form route-entry field.
4. Return to chart through the CDI/nav element.
5. Search/recenter on `KOLM`.
6. Drag the map.
7. Click the map, open inspector, select `KOLM`, and choose `Insert in flight plan`.
8. Return to the plan and verify `KOLM` is present.

Android path:

1. Verify the app is visible through UIAutomator.
2. Open the flight-plan page through the CDI/nav element.
3. Check whether the free-form route-entry field is present.
4. Return to chart through the CDI/nav element.
5. Tap the map surface.
6. Check whether the inspect tray and insert action are present.

Current Android gaps are reported as first-class journey output rather than hidden by platform-specific fallbacks. When the Android UI gains a parity-tagged free-form route-entry field and chart search, this same journey should be extended to perform the same full route as web.

## Stable Identifiers

Web uses `data-testid`. Android uses accessibility content descriptions prefixed with `parity:` so `adb shell uiautomator dump` can find the same semantic controls.

The first shared IDs are:

- `nav-cdi`
- `map-surface`
- `plan-append-route-input`
- `map-selection-tray`
- `map-selection-item:<label>` on Android and `map-selection-item-<category>-<label>` on web
- `map-selection-action:<id>` on Android and `map-selection-action-<id>` on web

Keep adding IDs at the view/controller boundary only. The harness should never duplicate app business logic.
