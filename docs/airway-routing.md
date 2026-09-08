# Airway routing

The first version edits one interval of the current flight plan. Select a
standalone waypoint row and choose **Find Route**. When exactly one later endpoint
is available, the map editor opens immediately. Otherwise, choose a later standalone
waypoint in the action tray; clicking the scrim dismisses that picker. The interval may contain complete airways, but no
procedures. Airway/procedure child rows cannot be boundaries. Applying a draft
replaces the interval as one undoable definition edit; cancel preserves the plan.
Draft via points are temporary editor state, not persistent routing history.

The search follows published low-altitude airways between ordered named via
points. Direct connectors are permitted only at the two interval endpoints when
they are outside the graph. The initial cost is total distance plus five times
direct distance. A candidate must traverse at least one airway edge. Direct
distance is a comparison; an unreasonable airway detour need not be offered.
Equal-distance solutions prefer fewer airway changes, including across pinned
fixes. The search retains the incoming airway at each fix so a locally tied
choice cannot force an unnecessary change later.

## NAV25 publication

NAV25 adds `airway/routing/manifest` (schema 1) and numbered
`airway/routing/chunk/00000` records. Each chunk contains at most 128 nodes with
directed adjacency lists. Node IDs are contiguous within one NAVDB publication;
they must not survive a NAVDB epoch change. Shared navigation identities join
airways; geometric crossings alone do not.

The intermediate database now retains AWY1 records in
`airway_segment_metadata`. Directional conventional and GNSS MEAs, maximum
altitudes, crossing constraints and signal-gap metadata accompany each graph
edge. Discontinued and unusable segments are excluded. A missing AWY1 record or
missing intermediate AWY2 position never establishes a connection. Unknown
altitudes remain null. The ordinary airway branch records remain unchanged.

MEAs describe the airway portion, not endpoint direct connectors. The route
tray's **VOR / GNSS** choice controls both eligible airway links and
the single altitude label shown on each leg. GNSS is the initial default; the
choice is remembered in device settings across editing activities and restarts.
GNSS mode prefers published GNSS MEAs, falling back to conventional MEAs when
needed. VOR mode uses conventional MEAs and excludes T routes, colored NDB
airways, RNAV-suffixed V routes, and links with only a GNSS MEA. Missing both
altitude fields remains explicitly unknown. Red leg labels and the route maximum
use the selected mode. MOCA and secondary altitude labels are not included.
Crossing constraints retain their separate meaning, including published context
at a fix where the pilot leaves the airway. Routing does not select procedures
or require an altitude ceiling from the pilot.

Smoke and release-journey fixture locks reference rebuilt NAV25 artifacts.
NAVDB rollover uses the permanent logical source in `crates/nav-db-fixture`,
which generates its scenarios locally without historical FAA cycles. See
[Hosted CI Invariants](testing/hosted-ci.md) for fixture ownership and checks.

## Map editor

The existing waypoint action tray offers **Find Route** beside **Add Airway**,
using the same airway symbol. A sole eligible endpoint starts routing immediately;
multiple eligible endpoints appear in a picker without a redundant Cancel button.
The map shows one suggested route in purple; its endpoint direct connectors
are dashed. Airway labels show the MEA, with
segments imposing the route's highest known MEA emphasized. Missing metadata
is explicitly marked unknown on the affected leg.
Published MCAs have separate callouts at their named crossing fixes, with source
airway/state encoding removed. A crossing fix outside the selected airway
portion gets no callout. Highest-MEA labels relocate around point labels, MCA
callouts, and the editor tray, with a leader to the affected leg when displaced.
They use the visible portion of a leg even when its midpoint is offscreen, and
are never dropped for a label collision. A continuous stretch with the same
airway and highest MEA shares one protected callout on its longest visible leg;
separate stretches retain separate callouts. Ordinary leg labels yield space first.
Placement uses measured text bounds in the displayed map frame; the core-owned
geometry has web/Android pointer-rate mirrors fenced by shared conformance cases.
Every airway change has an identifier and the existing navigation symbol for
that fix or navaid. A manually pinned junction keeps its purple circle and
identifier, avoiding a duplicate marker. These symbols share the editor's map
frame and do not enable map inspection during editing.

Drag the selected line to a named fix, navaid, or graph airport. The preview
snaps to the nearest graph node within the pointer's screen tolerance. Release
commits that temporary via point. Click a pin without dragging to unpin it, or
drag the pin to move it. **Undo** and **Redo** traverse the temporary editor's
pin, move, unpin, and navigation-mode history; a new edit after Undo discards its redo branch.
Switching navigation mode recomputes with the same endpoints and pins. If that
leaves no usable route, pins remain editable and Apply stays disabled. Drag
targets and endpoint graph connections use the selected navigation mode too.
A drop without a usable route restores the prior draft. Map gestures away from
the selected line continue to pan and zoom.

The compact tray shows the interval title and a distance comparison such as
**Direct: 1213 nm Airway: 1255 nm +3%**. Airway distance includes the endpoint
direct connectors; the percentage compares the full route with the direct
endpoint distance. Altitude information stays on the map. Its single row of
square icon buttons is **VOR, GNSS, Undo, Redo, Apply, Cancel**, with the shared
outer selection outline identifying the navigation mode. Error and live drag
feedback appear when needed.

**Apply** replaces the selected interval once. **Cancel** leaves the
flight plan unchanged. Reopening the same interval starts a new graphical editing
activity around its current boundaries. There is no saved routing history to
maintain after a manual flight-plan edit.
Procedures stop the list of eligible endpoints; routing can begin again at a
standalone waypoint after a procedure.

GNSS search uses published low airways (V, T, A, B, G, and R). VOR search uses
the eligible V routes. It considers
up to 24 entry/exit nodes within 75 NM for off-graph endpoints using the weighted
shortest path. An unpinned search suppresses large detours
relative to the direct distance; no useful airway route leaves Apply disabled.
An explicit via point permits a detour requested by the pilot.

## Focused verification

Fixture-free feature preflight, from `ui/core-rust` (create the empty artifact
directory first):

```sh
cargo +1.94.1 nextest run --locked --profile ci \
  -p app-core -p app-ui-contracts \
  -E 'test(airway) | test(routing_editor) | binary(ui_core_boundary) | package(app-ui-contracts)' \
  --config 'env.AEROBAG_ARTIFACT_READ_PATH="/tmp/aerobag-empty-route-tests"'
```

The ignored `published_nav25_airway_routes_pae_lgu_and_rnt_lgu_via_beezr` test
reads a full published NAV25 directory through the shared directory reader.
Set `AEROBAG_ROUTING_NAV_DIR` to that directory and run the test with
`--ignored --nocapture`. It checks routes from PAE, KPAE, and KRNT to KLGU,
with and without BEEZR. It also checks KRNT–KLVM via OCS and MBW in both navigation modes, including
V4's 16,000 ft conventional / 11,700 ft GNSS MEA near HODNI.
Synthetic tests cover direct penalties, disconnected and coincident nodes,
ordered pins, missing pages, MEAs, stale actions,
procedure boundaries, interval isolation, and one-step Apply/Undo.

The browser walkthrough is: enter `KRNT KLGU`; open the KRNT row's **Find Route**
action to open the map immediately; zoom to the Cascades; drag V4 to BEEZR; click to unpin;
undo and redo; undo again; pan and verify the pin follows BEEZR; apply;
reopen the same interval; cancel; undo the flight-plan edit. Use the semantic
driver's rendered geometry to locate map gestures. The expected first BEEZR
route begins with V2/V298 and the final flight-plan Undo restores the original
two-waypoint plan.

## Map modes and geographic layers

Core projects `map_interaction` with two modes: Explore permits inspection and
weather hover; FindRoute permits graphical route editing. Both modes retain
map navigation. Platforms attach handlers using the projected permissions,
clear existing inspection UI on mode changes, and reject late inspection
results. Search can still center on a fix while the inspection tray is disabled.
Starting **Find Route** again intentionally replaces the previous activity.

Web geographic editor vectors use `MapGeometryBinding` and `MapGeometryLayer`
to enter the map's existing bearing/content transform. Their coordinates use
the committed north-up frame, while pointer conversion reads the live display
frame. This lets the immediate drag transform move the editor with raster and
vector layers before React commits the next viewport. The tray stays in screen
coordinates. Android passes the shared live `MapDisplayFrame` to the editor
for drawing, hit testing, and pointer conversion. The boundary suite checks
these integration points so a future overlay cannot silently bypass them.
