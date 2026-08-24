# Product Release User-Journey Plan

Status: implemented locally; hosted CI awaits publication of the pinned fixture
Audited revision: `927d274aa230`
Audit date: 2026-08-20

## Implementation Snapshot

The release harness now provides:

- a machine-validated journey registry and product-surface coverage manifest;
- shared semantic web/Android drivers and structured result transcripts;
- complete shared P0, P1, and P2 journey implementations, plus the existing
  platform resilience tests;
- a capability-addressed compact publication fixture committed in
  `aerobag/test-artifacts` as
  `8940c4d7f3cb8e42410c2f64f454fe5839791109`;
- one optimized web build, one release-like Android APK, and one Cloud test
  server built per candidate commit and shared by all journey jobs; and
- P0/P1 web and Android matrices on pushes and pull requests, with P2 nightly
  and all priorities available through manual dispatch.

The fixture commit is present in the shared local test-artifacts mirror. This
machine's GitHub deploy key can read but cannot push that repository, so hosted
CI remains blocked until the same commit is published to the GitHub mirror.

Local verification on 2026-08-23 completed with:

- 53 release-journey foundation tests passing;
- 108 product branches covered across 13 machine-checked categories;
- every implemented shared P0/P1/P2 journey passing on the optimized web
  bundle;
- every implemented shared P0/P1/P2 journey passing on the immutable Android
  APK, including package maintenance and contract-failure coverage; and
- platform-specific web pointer behavior and Android package maintenance
  passing.

The stable local entry point is `tools/e2e/release_journey_lab.sh`. Build,
fixture, emulator, Cloud, web, and Android operations are subcommands of that
single wrapper so one narrowly scoped persistent command approval covers lab
iteration without approving changing low-level commands individually.

## Goal

A product release should be blocked when a pilot can no longer reach or use an
important feature through the shipped UI, even when the underlying core unit
tests still pass. The release suite should exercise the same semantic journeys
on web and Android wherever the product behavior is shared, and should add
platform-specific journeys where the platform capability is intentionally
different.

This is UI feature-branch coverage, not data-set coverage. One representative
georeferenced plate is enough to prove that georeferenced plates work; every
plate does not need an end-to-end test. The representative must still exercise
the complete pilot-visible contract: reach it, open it, scroll and zoom it,
observe ownship when applicable, and read applicable warnings and NOTAMs.

## What Counts As Coverage

A journey covers a branch only when it:

1. Reaches the feature through controls a pilot can use in the shipped app.
2. Performs the action, rather than merely inventorying the control.
3. Observes the resulting UI or rendered output.
4. Checks the important alternate state, such as on/off, enabled/disabled,
   connected/disconnected, or available/unavailable.
5. Runs against the packaged web application or installed Android APK and a
   contract-valid publication fixture.

Control inventories are valuable parity fences, but an inventory does not
prove that a button works. Core-only tests remain the right place for exhaustive
algorithms and data combinations; journeys prove the platform-to-core-to-UI
path.

## Existing Journey Audit

### Release CI today

`.github/workflows/e2e-ci.yml` runs on every push and pull request. It currently
contains these independently reported jobs:

- **`android.flight-plan-route-smoke`**
  - **Platform:** Android
  - **Proves:** Accept/start, sync fixture packages, enter `KRNT KPWT`, open
    Chart, center destination, and observe a painted route segment.
  - **Limit:** Does not edit the plan or exercise row actions.
- **`android.plate-first-render-smoke`**
  - **Platform:** Android
  - **Proves:** Search `KPLU`, open Plates from the inspector, select the first
    folder tile, and pixel-check the first plate paint.
  - **Limit:** Does not pan, zoom, scroll pages, show ownship, or inspect
    warnings.
- **`android.map-follow-ctr-gesture-smoke`**
  - **Platform:** Android
  - **Proves:** Use deterministic ownship, engage CTR, drag and repeatedly zoom
    while preserving follow and offset.
  - **Limit:** Does not cover TRK-up or loss of track.
- **`android.layer-toggle-navdb-regression`**
  - **Platform:** Android
  - **Proves:** Turn Terrain Warning off and NEXRAD on and verify
    core-projected state changes.
  - **Limit:** Only two of seven layer branches; no rendered-content assertion.
- **`android.rotation-session-retention-regression`**
  - **Platform:** Android
  - **Proves:** Preserve an active route, route paint, process, and a persisted
    NOTAM through repeated portrait/landscape recreation.
  - **Limit:** A deep resilience test, not broad feature coverage.
- **`android.chrome.live-feed-network-recovery`**
  - **Platform:** Chrome on Android
  - **Proves:** Advance METAR `v1` to `v3`, go offline, restore online,
    reconnect promptly, and retain one EventSource.
  - **Limit:** Web transport only; does not prove native Android feed display
    or other products.
- **`web.nav-db-rollover`**
  - **Platform:** Web
  - **Proves:** Adopt a good next NAVDB while preserving a rich plan; reject a
    bad NAVDB and expose the real reload warning.
  - **Limit:** Specialized lifecycle coverage, not ordinary UI coverage.

The Android raw-terrain inspector journey exists but is omitted from hosted CI
because it needs the full terrain publication:
`android.raw-map-inspector-terrain-smoke` selects a raw SPOT and checks terrain
elevation.

### Existing local-only journeys

These are useful assets, but they do not currently protect a release in hosted
CI:

- **`tools/parity/run-flight-plan-inspect-journey.mjs`**
  - **Coverage:** One semantic flow on web and Android: CDI navigation, route
    entry and feedback, first-tap weather, action inventories, map family/layer
    inventories, plate-control inventories, one plate procedure load attempt,
    map drag/search/inspect, and airport insertion.
  - **Gap:** Mostly inventories action classes; it executes only a few. It is
    not a CI job and still records an inspect-footprint divergence.
- **`browser-platform-smoke.mjs`**
  - **Coverage:** Shared time-mode actions, TRK-up raster-corner coverage,
    narrow ownship rendering, SPOT elevation/coordinates, and live distance
    refresh.
  - **Gap:** Web only and local only.
- **`settings-debug-e2e.mjs`**
  - **Coverage:** Home navigation, no legacy DBG tray, folded Debug
    Diagnostics, and one core round-trip toggle.
  - **Gap:** Web only, local only, and one Settings row.
- **`disclaimer-persistence-e2e.mjs`**
  - **Coverage:** Accept disclaimer, verify core settings persistence, and
    reload without seeing it again.
  - **Gap:** Web only and local only.
- **`cloud-page-smoke.mjs`**
  - **Coverage:** Open Cloud, create an account, and observe projected page
    states.
  - **Gap:** Web only and local only.
- **`aerobag-cloud-sse-e2e.mjs`**
  - **Coverage:** Link two browser profiles, crossfill a plan and package
    preferences, reconnect a dropped stream, and preserve later updates;
    separate rate-limit UX modes.
  - **Gap:** Web-to-web only and local only.
- **`wasm-startup-smoke.mjs`**
  - **Coverage:** Import and initialize the generated WASM adapter.
  - **Gap:** Build smoke, not a user journey.

The parity harness's stable `data-testid` and Android `parity:` identifiers are
the right foundation. The next suite should reuse them, while replacing broad
“control exists” claims with focused action-and-effect assertions.

## Product Surface Audit

Priority meanings:

- **P0**: loss makes the app unusable or removes a primary in-flight workflow.
  It must gate every release candidate.
- **P1**: major advertised capability or important safety/status information.
  Add it to the release gate as its journey is implemented.
- **P2**: resilience, administrative, debug, or less-common branch. Run nightly
  and require a green run at the exact release commit before promotion.

“Current” names the strongest existing end-to-end evidence. Unit and source
policy tests are intentionally not counted here.

### Startup And Global Navigation

- **First launch disclaimer and acceptance persistence**
  - **Expected:** Accept reaches a usable chart; reload does not ask again.
  - **Current:** Android tests accept if present; web persistence is local-only.
  - **Needed:** Shared fresh-profile journey with persistence assertion.
  - **Priority:** P0
- **Supported-publication startup**
  - **Expected:** Resolve the publication and paint initial raster and vector
    content.
  - **Current:** Indirect in Android tests; no explicit complete-display
    assertion.
  - **Needed:** Assert shell dismissal, map input readiness, raster paint,
    vector paint, and no startup error.
  - **Priority:** P0
- **Bottom navigation graph**
  - **Expected:** Reach Home, Plan, the Chart/Plate return target, and Altitude
    Planner from every applicable page.
  - **Current:** Parity harness checks only CDI/Home/Chart/Plate portions.
  - **Needed:** Navigate the full graph on both platforms and assert the active
    page each time.
  - **Priority:** P0
- **Home destinations**
  - **Expected:** Every enabled destination opens: Chart, Plate, Flight Plan,
    Altitude Planner, Status, Settings, Cloud when supported, Offline Packages
    on Android, and About.
  - **Current:** Settings and Cloud have web-only smokes.
  - **Needed:** Inventory from core, execute every enabled destination, and
    verify disabled reasons for unsupported destinations.
  - **Priority:** P1
- **Unsupported contract/publication startup**
  - **Expected:** Produce a readable failure instead of a blank or crashed app.
  - **Current:** NAVDB reject covers only a running-session advance.
  - **Needed:** Add cold-start fixture variants for unsupported UI,
    publication, NAVDB, and live-feed contracts.
  - **Priority:** P2
- **Saved-state restart**
  - **Expected:** Saved page, viewport, and settings survive without trapping
    the user on an unreachable surface.
  - **Current:** Android rotation preserves a live session only.
  - **Needed:** Restart with saved Chart and Plate state plus a recovery route
    to Home.
  - **Priority:** P2

### Chart And Map

- **Pan and zoom**
  - **Expected:** Pan reveals newly painted raster/vector content; zoom changes
    scale without losing interaction.
  - **Current:** Map drag inventory and Android CTR zoom.
  - **Needed:** Assert viewport movement, new raster paint, new vector
    projection, and bounded gray exposure.
  - **Priority:** P0
- **Raster-family selection**
  - **Expected:** `none`, Sectional, TAC-over-Sectional,
    FLYWAY-over-Sectional, IFR-L, IFR-H, and Shaded Relief visibly change the
    plan; `none` paints no chart/world tiles.
  - **Current:** Family options are only inventoried.
  - **Needed:** Select every fixture family and assert selected state plus a
    family-specific painted-source probe.
  - **Priority:** P1
- **Layer toggles**
  - **Expected:** World Basemap, Vectors, METARs, NEXRAD, Traffic, Terrain
    Warning, and Offline Regions change visible state; disabled reasons remain
    readable.
  - **Current:** Android toggles Terrain/NEXRAD; parity inventories four
    historical layers.
  - **Needed:** Execute every available toggle, assert core state and a
    semantic render probe, and test one disabled branch.
  - **Priority:** P1
- **Chart search**
  - **Expected:** Find an airport, recenter it, and leave it inspectable.
  - **Current:** Parity harness covers `KBFI`.
  - **Needed:** Promote to a release journey and assert center/selection, not
    only the suggestion click.
  - **Priority:** P0
- **CTR follow**
  - **Expected:** Drag/zoom preserve the intended offset while enabled;
    disabling CTR allows free pan.
  - **Current:** Android covers enabled drag/zoom.
  - **Needed:** Add the disabled branch and web parity.
  - **Priority:** P0
- **N-up/TRK-up**
  - **Expected:** Track rotates map and compass; missing TRK preserves the last
    orientation.
  - **Current:** Web smoke checks rotated raster corners; Android not covered.
  - **Needed:** Shared replay-backed journey with changing track and a
    deliberate track gap.
  - **Priority:** P1
- **Ownship source and status**
  - **Expected:** Open the situation/source panel; selecting an available
    source changes status and drives MSL/AGL/TRK/GS plus the aircraft symbol.
  - **Current:** Android CTR tests select only the debug Bad Autopilot source.
  - **Needed:** Shared deterministic source-selection journey with connected
    and unavailable branches.
  - **Priority:** P1
- **Registered overlays**
  - **Expected:** Ownship, flight-plan route, labels, weather discs, terrain
    warning, and overlays stay registered to the chart during pan/rotation.
  - **Current:** Route and CTR probes are partial.
  - **Needed:** Replay-backed screenshot/probe assertions before and after
    pan/rotate.
  - **Priority:** P1
- **Chart status badges**
  - **Expected:** Global and procedure `/!\` badges open readable detail and
    actions.
  - **Current:** NAVDB reject covers one global warning.
  - **Needed:** Fixture one global warning and one loaded-procedure geometry
    warning on Chart.
  - **Priority:** P1
- **TAC reference accessory**
  - **Expected:** Open Plate references without leaving the basemap tray stuck
    open.
  - **Current:** None.
  - **Needed:** Select TAC, invoke the reference icon, and assert the Plate
    reference folder and closed map tray.
  - **Priority:** P1

### Map Inspector And Detail Modals

- **Inspector preselection**
  - **Expected:** A map click opens an inspector and preselects the nearest
    visible airport, otherwise SPOT.
  - **Current:** Parity opens an airport; local tests cover SPOT.
  - **Needed:** Deterministic airport and empty-ground clicks on both
    platforms.
  - **Priority:** P0
- **Selected-point facts**
  - **Expected:** Display current distance, elevation, and lat/lon; distance
    follows ownship; SPOT terrain arrives asynchronously.
  - **Current:** Web-local distance/elevation; Android terrain local-only.
  - **Needed:** Shared replay-backed distance update plus a fixture-backed
    asynchronous terrain page fault.
  - **Priority:** P1
- **Airport inspector actions**
  - **Expected:** Direct, Insert, WX, Info, and Plates execute; disabled actions
    explain why.
  - **Current:** Parity executes Insert and first-tap WX from Plan.
  - **Needed:** Execute one of each through the inspector and verify its plan,
    navigation, or modal effect.
  - **Priority:** P0
- **Weather modal**
  - **Expected:** Present METAR, TAF, and airport NOTAM text, freshness/empty
    states, selectable text on web, and close cleanly.
  - **Current:** First-tap presence only.
  - **Needed:** Assert representative content and a missing-TAF branch on both;
    add a web selection/copy smoke.
  - **Priority:** P1
- **Airport Info modal**
  - **Expected:** Show friendly name/location, time and sun facts, comms, TPA,
    distance/elevation, and runway complex/fallback diagrams; time basis
    toggles globally.
  - **Current:** None.
  - **Needed:** Use one complex airport and one incomplete/small airport,
    scroll the entire modal, toggle time, and inspect the final section.
  - **Priority:** P1
- **Other inspectable feature classes**
  - **Expected:** Airspace, TFR, PIREP, obstacle, and nav-aid features select,
    highlight, and expose readable detail/limits where applicable.
  - **Current:** None.
  - **Needed:** One representative of each distinct detail renderer and one
    overlapping-feature selection.
  - **Priority:** P2
- **Web METAR hover**
  - **Expected:** Hover opens weather; moving away closes it without changing
    selection.
  - **Current:** None.
  - **Needed:** Web-only pointer journey.
  - **Priority:** P2

### Flight Plan And Navigation

- **Free-form route entry**
  - **Expected:** A valid route commits; an invalid route shows feedback and
    remains editable.
  - **Current:** Parity covers both.
  - **Needed:** Promote to CI and assert exact resulting rows.
  - **Priority:** P0
- **Flight-plan row mutations**
  - **Expected:** Insert before/after, best-position Insert, move, remove, and
    remove-all-above mutate the intended rows.
  - **Current:** Only best-position Insert executes; row actions are
    inventoried.
  - **Needed:** Execute each mutation on a short deterministic route and assert
    row order.
  - **Priority:** P0
- **Airway picker**
  - **Expected:** Open from an airway-capable fix, scroll on narrow Android,
    select an exit, and insert the airway expansion.
  - **Current:** None.
  - **Needed:** Shared route journey with Android scroll assertion.
  - **Priority:** P1
- **Active navigation controls**
  - **Expected:** Direct-To, Activate Leg, Next Leg, Sequence,
    Suspend/Unsuspend, Stop, and Restore Direct-To update active guidance and
    route paint.
  - **Current:** Controls mostly inventoried.
  - **Needed:** Execute a compact state-machine journey and assert projected
    active leg/guidance after each step.
  - **Priority:** P0
- **Undo and Redo**
  - **Expected:** Restore flight-plan mutations; web keyboard shortcuts behave
    like the buttons.
  - **Current:** None.
  - **Needed:** Shared button journey plus a web-only shortcut branch.
  - **Priority:** P1
- **Flight-plan estimates**
  - **Expected:** ETA/ETE/fuel/distance rows appear, ETE toggles
    leg/cumulative, time toggles Z/local globally, and estimates survive vector
    discontinuities.
  - **Current:** Web local covers time toggles only.
  - **Needed:** Representative modeled route containing a vector discontinuity
    and action assertions.
  - **Priority:** P1
- **Flight-plan weather**
  - **Expected:** Weather badges appear on fresh airport rows, WX opens in one
    tap, and stale weather does not appear fresh.
  - **Current:** First-tap modal is covered locally by parity.
  - **Needed:** Add a fresh/stale live-feed fixture and visible badge
    assertions.
  - **Priority:** P1
- **Mutation invariant explanations**
  - **Expected:** Invalid actions are disabled with readable reasons instead of
    silently doing nothing.
  - **Current:** Inventories record disabled state, not help text.
  - **Needed:** Attempt protected origin/destination mutations around
    procedures and assert the explanation.
  - **Priority:** P1
- **Rotation retention**
  - **Expected:** Portrait/landscape recreation retains the plan, active leg,
    and route.
  - **Current:** Android CI covers this deeply.
  - **Needed:** Keep the existing regression.
  - **Priority:** P0

### Procedures: SID, STAR, And Approach

Each procedure kind is a separate feature branch. An Approach test does not
cover SID or STAR UI, even when the core representation is shared.

- **SID selection and use**
  - **Expected:** Select Departure at origin, choose procedure and transition,
    render it, expose SHOW PLATE, and remove it.
  - **Current:** None.
  - **Needed:** Shared SID journey using a multi-leg/vector example.
  - **Priority:** P0
- **STAR selection and use**
  - **Expected:** Select Arrival at destination, choose procedure and
    transition, render it, expose SHOW PLATE, and remove it.
  - **Current:** None.
  - **Needed:** Shared STAR journey using a multi-page plate.
  - **Priority:** P0
- **Approach selection and use**
  - **Expected:** Select Approach or load from Plate, choose the IAF transition,
    render it, expose SHOW PLATE, and replace/remove it.
  - **Current:** Parity attempts one Plate load; not release CI.
  - **Needed:** Shared approach journey through both Plan and Plate entry
    points.
  - **Priority:** P0
- **Procedure discontinuities**
  - **Expected:** Vector/manual discontinuities use chevrons, carry no bogus
    distance pill, and estimates bridge sensibly to the next fix.
  - **Current:** None.
  - **Needed:** SID/STAR fixtures with vectors and explicit route/estimate
    probes.
  - **Priority:** P1
- **Procedure geometry warning**
  - **Expected:** A loaded Chart route shows the pilot-friendly procedure name
    and readable warning details.
  - **Current:** None.
  - **Needed:** Known warning fixture and `/!\` tray assertion.
  - **Priority:** P1
- **Procedure mutation invariants**
  - **Expected:** Procedures remain attached to the first/last airport and
    blocked moves/removals explain why.
  - **Current:** None.
  - **Needed:** Extend procedure journeys with one forbidden operation per
    kind.
  - **Priority:** P1

### Plates, Airport Documents, Legends, And Insets

- **Airport selector sections**
  - **Expected:** Selected, Departure, Arrival, Plan, and Recent appear when
    nonempty; Recent is bounded.
  - **Current:** Parity inventories one airport tray.
  - **Needed:** Build route/recent state and assert ordered section IDs and
    reachable entries.
  - **Priority:** P1
- **Folder selection**
  - **Expected:** Thumbnails open the chosen chart, suggested charts are
    highlighted, and the user can return to the folder.
  - **Current:** Android checks the first arbitrary tile paint.
  - **Needed:** Select a named chart, assert identity, return, and assert the
    suggestion highlight.
  - **Priority:** P0
- **Georeferenced plate operation**
  - **Expected:** A single-page plate pans/zooms and displays registered
    ownship; the debug flight-plan overlay can be tested separately.
  - **Current:** First paint only.
  - **Needed:** Replay-backed journey with pan/zoom/registration probes.
  - **Priority:** P0
- **Multi-page rotated plate operation**
  - **Expected:** Scroll from first to last automatically rotated page and zoom
    without clipping.
  - **Current:** None.
  - **Needed:** Use a two-page rotated STAR and assert both page-paint probes.
  - **Priority:** P0
- **Plate advisories**
  - **Expected:** The NOTAM badge opens detailed publication-sourced NOTAMs;
    the geometry badge opens publication warning text.
  - **Current:** None.
  - **Needed:** One fixture chart for each badge provenance, including readable
    final text.
  - **Priority:** P1
- **Plate procedure loading**
  - **Expected:** LOAD DEPARTURE/ARRIVAL/APPCH has the correct append/replace
    header and mutates the plan.
  - **Current:** One approach load attempt.
  - **Needed:** Exercise one non-destructive append and one destructive replace
    confirmation.
  - **Priority:** P1
- **Chart references**
  - **Expected:** TAC/SEC/IFR legend folders open, TAC viewport suggestions
    highlight applicable insets, and each composite scrolls fully.
  - **Current:** None.
  - **Needed:** One legend family and one TAC inset journey, then a renderer
    inventory for remaining families.
  - **Priority:** P1
- **Other airport documents**
  - **Expected:** A CSup/airport diagram and ordinary non-procedure document
    are reachable and scroll/zoom like plates.
  - **Current:** None.
  - **Needed:** One representative of each distinct document collection.
  - **Priority:** P2

### Altitude Planner And Flight Data

- **Altitude Planner reachability**
  - **Expected:** Home and bottom navigation reach the planner with a route
    loaded.
  - **Current:** Home icon checked web-only.
  - **Needed:** Shared reachability journey.
  - **Priority:** P1
- **Aircraft and profile selection**
  - **Expected:** Selectors change labels, ownship plan view, and computed
    comparison values.
  - **Current:** None.
  - **Needed:** Select two materially different aircraft/profiles and assert
    output changes.
  - **Priority:** P1
- **Wind-model selection and fallback**
  - **Expected:** Forecast/no-wind choice and available forecast change
    comparison values; missing/out-of-coverage forecast shows a nonblocking
    advisory.
  - **Current:** None.
  - **Needed:** Deterministic winds fixture plus missing-page and
    outside-validity branches.
  - **Priority:** P1
- **Departure-time editing**
  - **Expected:** Time edits and global Z/local mode update the model without a
    calculating blink when inputs are unchanged.
  - **Current:** None.
  - **Needed:** Edit time, toggle basis from another surface, and assert stable
    result/refresh behavior.
  - **Priority:** P2
- **Altitude selection and unavailable state**
  - **Expected:** Altitude choices update the selected row and plan estimates;
    unavailable inputs explain why planning is disabled.
  - **Current:** None.
  - **Needed:** One normal comparison and one missing-origin/aircraft branch.
  - **Priority:** P1
- **Configurable flight-data cells**
  - **Expected:** Cells hide/show in Chart and Plan consistently; live
    MSL/AGL/TRK/GS and ETA fields update.
  - **Current:** Time fields web-local only.
  - **Needed:** Settings-to-Chart/Plan shared journey with replay ownship and
    terrain.
  - **Priority:** P1

### Status, Settings, Cloud, Replay, And Offline Packages

- **Status-page completeness**
  - **Expected:** Reach and scroll through client, publication, NAVDB,
    chart/docs/static packages, live-feed connection, and every listed product.
  - **Current:** A persisted NOTAM row is checked during Android rotation.
  - **Needed:** Deterministic full-status fixture; assert row IDs,
    representative facts, and last-row reachability.
  - **Priority:** P1
- **Live-feed status transitions**
  - **Expected:** Fresh/stale/missing states produce the correct severity and
    readable timestamps; the Status time toggle follows global mode.
  - **Current:** Live-feed reconnect checks METAR version only.
  - **Needed:** Fixture transitions for fresh, old, missing, and recovered
    products.
  - **Priority:** P1
- **Settings controls**
  - **Expected:** Flight-data grid changes visible cells; Android dim/sleep
    sliders round-trip; Debug Diagnostics starts folded and toggles a
    representative flag.
  - **Current:** Debug web-local only.
  - **Needed:** Shared grid/debug journey and Android-only display-policy
    journey.
  - **Priority:** P1 for grid; P2 for debug/display policy.
- **Replay**
  - **Expected:** Load a trace, play/pause, change rate, seek, display gaps,
    drive ownship, and survive track gaps.
  - **Current:** None.
  - **Needed:** Shared deterministic trace journey.
  - **Priority:** P1
- **Cloud lifecycle and crossfill**
  - **Expected:** Create, link, crossfill, reconnect, and unlink work on both
    clients.
  - **Current:** Strong web-to-web local harness.
  - **Needed:** Promote the fixture, add at least one Android participant, and
    cover unlink.
  - **Priority:** P1
- **Android offline cold start**
  - **Expected:** Clean install refreshes Offline Packages, changes
    region/product/zoom choices, syncs with progress, closes, and uses data
    after airplane-mode restart.
  - **Current:** Package sync is setup for Android tests, not their asserted
    subject.
  - **Needed:** Dedicated Android journey with offline restart and map/plate
    use.
  - **Priority:** P0
- **Android package maintenance**
  - **Expected:** Update plan fetches new, retains paused, and deletes obsolete
    packages; failure/cancel remains recoverable.
  - **Current:** None.
  - **Needed:** Two-cycle package fixture with fetch/keep/pause/delete and an
    interrupted sync.
  - **Priority:** P2
- **About**
  - **Expected:** Reach the page and scroll its content.
  - **Current:** None.
  - **Needed:** One simple shared reachability journey.
  - **Priority:** P2

## Proposed Journey Set

Keep journeys small enough that one failure names one broken workflow. Reuse a
shared semantic journey definition with web and Android drivers instead of
growing one monolithic parity script.

### P0 release gate

1. `shared.startup-navigation`: fresh start, disclaimer, complete initial map,
   Home/CDI/Chart/Plate/Plan navigation, persisted acceptance.
2. `shared.chart-basic-use`: search, pan, zoom, raster/vector repaint, inspect,
   CTR on/off, and return navigation.
3. `shared.flight-plan-edit-and-navigate`: route entry, invalid feedback,
   insert/move/remove, Direct-To/Activate/Sequence/Suspend, and route paint.
4. `shared.procedure-departure`: choose, render, show plate, enforce one
   invariant, and remove a SID.
5. `shared.procedure-arrival`: choose, render, show a multi-page plate, enforce
   one invariant, and remove a STAR.
6. `shared.procedure-approach`: load from Plan and Plate, render, replace, show
   georeferenced plate, and remove an approach.
7. `shared.plate-operate`: named folder selection, first/last page, pan, zoom,
   georeferenced ownship, and return to folder.
8. `android.offline-cold-start`: select/sync packages, kill app, disable
   network, restart, and use Chart plus Plate.
9. Keep `android.rotation-session-retention-regression` and both
   `web.nav-db-rollover` scenarios as P0 resilience jobs.

### P1 release gate

1. `shared.map-modes-and-overlays`: every raster family, every available layer,
   N/TRK with a track gap, warning launcher, and chart-reference handoff.
2. `shared.inspector-details`: airport/spot priority, live distance, terrain,
   WX, Info, Plates, and one disabled action reason.
3. `shared.airport-info`: complete scroll, time toggle, published/derived TPA,
   runway complex, and fallback runway.
4. `shared.flight-plan-airway-estimates`: airway picker, estimates across
   vectors, ETE scope, global time mode, weather badge, Undo/Redo.
5. `shared.plate-advisories-and-references`: NOTAM, geometry warning, legend,
   inset suggestion, and composite scroll.
6. `shared.altitude-planner`: aircraft/profile/wind/time/altitude choices,
   changed estimates, forecast fallback, and explanatory unavailable state.
7. `shared.status-and-settings`: all expected row IDs, stale/missing state,
   flight-data visibility, and folded Debug Diagnostics.
8. `shared.replay-track-up`: load/play/rate/seek, ownship movement, map rotation,
   and gap handling.
9. `shared.cloud-crossfill`: create/link web and Android, crossfill plan and
   package preferences, reconnect, and unlink.
10. Split live-feed tests by distinct ingestion/rendering path:
    `shared.prepared-live-feeds`, `shared.nexrad-frames`,
    `shared.obstacles-navkv`, `shared.winds-aloft-navkv`, and
    `shared.tfr-map-detail`.

### P2 extended release gate

Add cold-start contract failures, two-cycle Offline Package maintenance,
network interruption during package sync, web-only METAR hover/copy, uncommon
inspector detail renderers, CSup/non-procedure documents, Android display sleep
policy, About, and saved-page/viewport recovery.

## Fixture Plan

Create one pinned `release-journey-publication` in `aerobag/test-artifacts`,
separate from the current minimal Android smoke publication. It should remain
small but contain capabilities, not arbitrary “first records”:

- all seven raster-family branches over one compact geographic area, including
  one TAC with a legend and viewport-specific inset;
- vectors, terrain, shaded relief, world basemap, and geodesy for that area;
- airports supporting: runway complex, incomplete/fallback runway, published
  TPA, derived TPA, airway, SID, STAR, approach, georeferenced plate,
  multi-page rotated plate, plate NOTAM, and procedure geometry warning;
- one CSup and one non-procedure airport document;
- deterministic fresh and stale METAR/TAF/NOTAM, TFR, NEXRAD, obstacle, traffic,
  and winds-aloft states;
- a short replay trace with movement, altitude, changing track, and a track
  gap;
- a second publication cycle for package maintenance and NAVDB rollover.

The fixture manifest should identify records by capability, for example
`georeferenced_plate`, rather than making runners rediscover “the first plate”.
Fixture construction should fail if a required capability disappears. Keep
large source-data correctness tests in fixture/preprocessor CI; the journey
fixture contains only the cooked outputs needed by the UI.

## Harness Design

1. Extract semantic operations from the current parity runner into a shared
   driver interface: `openPage`, `chooseOption`, `inspectMapAt`, `performAction`,
   `drag`, `zoom`, `readProjection`, and `captureFrame`.
2. Keep web DOM/CDP and Android adb/UIAutomator mechanics in platform drivers.
   Do not duplicate core sorting, eligibility, or labels in the harness.
3. Use core-derived stable IDs for controls and records. A feature without a
   stable ID is test debt. Add semantic render probes for canvas-only facts such
   as painted source IDs, visible route segments, ownship registration, and
   plate page bounds.
4. Use screenshot/pixel assertions only for facts inaccessible to semantic
   probes: nonblank raster/plate paint, clipping, registration, and gross
   overlap. Save screenshots for every failed checkpoint.
5. Reset app state per journey. Share the built APK, web bundle, emulator, and
   publication server, but not flight plans, preferences, or browser profiles.
6. Give every readiness wait a deadline and process-exit diagnostics. Do not
   weaken behavioral assertions to absorb slow CI.
7. Record a structured transcript containing action IDs, before/after core
   projection summaries, timings, console/logcat exceptions, and artifact
   paths.

### Findings to follow up

- Waypoint search must rank an exact normalized identifier ahead of prefix or
  substring matches regardless of distance. Add a core regression proving a
  query for `27W` selects `27W`, not `27WA`.
- Platform drivers must resolve actionable semantic IDs exactly. Prefix
  matching is reserved for state-bearing projection IDs whose suffix is part
  of the assertion; otherwise an action for `27W` can silently target `27WA`.
- Android raster loading can settle at `17/18` cache slots with `failed:0`
  after rapid chart-family changes even though the visible viewport is fully
  painted. The release journey gates on painted coverage, but the unaccounted
  planner/loader slot should be diagnosed separately.

## Preventing Coverage Drift

Add a machine-readable product-surface manifest alongside the runners. It
should map each core-projected branch to one of:

- a journey assertion ID;
- a deliberate platform-specific exclusion with a reason; or
- `uncovered`, which fails the coverage-manifest test.

At minimum, mechanically reconcile these evolving sets:

- Home and navigation destinations;
- raster map-family IDs and map-layer IDs;
- flight-plan global controls and row-action IDs;
- procedure kinds;
- Settings rows and Debug flags;
- Status row/product IDs;
- plate selector/load control classes;
- altitude-planner controls;
- Cloud action IDs.

This does not prove behavior by itself. It prevents a new core-projected button
or enum branch from silently falling outside the journey plan.

## Release Execution

Build once per commit, then fan out journeys:

1. Build the optimized production web bundle and release-like signed Android
   APK once. Upload both as immutable workflow artifacts.
2. Fetch and verify the pinned release-journey publication once.
3. Run each journey as an independently named matrix job against those exact
   binaries and fixture hashes.
4. Run shared P0/P1 journeys on both web and Android. Compare semantic outputs
   after both platform runs, but retain each platform result independently.
5. Run platform-specific and resilience jobs separately.
6. Upload transcript, browser console/logcat, final state, screenshots, and
   fixture/build identities for 14 days or longer for promoted releases.

P0 and implemented P1 jobs run on pull requests and every release candidate.
P2 runs nightly; promotion requires a green P2 run at the exact candidate
commit. A behavior failure is never hidden by retry. One infrastructure retry
is acceptable only when the first run proves an emulator/browser/setup failure
before the first application assertion.

## Implementation Order

1. **Foundation:** create the release fixture contract, shared driver API,
   structured result schema, build-once workflow jobs, and product-surface
   manifest. Move the current parity runner into this structure without losing
   checks.
2. **Primary workflows:** implement the P0 startup, chart, flight-plan,
   SID/STAR/approach, plate, and Android offline journeys. Promote existing
   rotation and NAVDB rollover jobs unchanged.
3. **Rich product branches:** implement P1 map modes, details, references,
   altitude planning, status/settings, replay, cloud, and product-specific
   live-feed journeys.
4. **Resilience:** add P2 contract, network, package-maintenance, and
   platform-only cases.
5. **Release policy:** make P0/P1 required checks and require same-commit P2
   evidence in the production deployment workflow.

The first milestone is complete when a release cannot ship with Chart, Plan,
SID, STAR, Approach, Plate, or Android offline use unreachable. The final
milestone is complete when every branch in the product-surface manifest is
either exercised through a shipped UI or explicitly excluded with a reviewed
reason.
