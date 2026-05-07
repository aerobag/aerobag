# Web/Android Action Parity

The parity runner should exercise action classes, not incidental labels. Web is
allowed to be the first implementation surface, but every reachable action class
must either be reachable on Android or appear as an explicit parity gap.

Current runner:

```sh
node tools/parity/run-flight-plan-inspect-journey.mjs both --url http://127.0.0.1:8082/ --serial emulator-5554
```

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

When adding a new reachable web action class, add a parity tag/test id and add
the matching Android journey assertion in the same change. A missing Android
implementation should be recorded as a journey gap instead of silently falling
out of coverage.
