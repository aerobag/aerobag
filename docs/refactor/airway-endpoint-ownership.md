# Airway Endpoint Ownership

Airways bind explicit, adjacent top-level waypoint occurrences in core. For example:

```text
KRNT
BANDR
V2 [BANDR -> ELN]
  intermediate fixes only
ELN
V187 [ELN -> PSC]
  intermediate fixes only
PSC
```

`AirwaySegment<FlightPlanWaypointId>` references the entry and exit's stable route-component
UIDs. The waypoints own their `NavRef`s. ELN is one shared occurrence, not two airway
children that a projector deduplicates. Two separate visits to ELN retain separate IDs.
`AirwaySegment<NavRef>` is a transient NAVDB materialization request, not a stored plan.

## Mutations

- Removing or moving a referenced endpoint is rejected. Menu buttons are disabled with
  help from the same core structural validation used by mutations.
- Inserting between an airway and either endpoint is rejected. An airway header cannot
  move away from its endpoints.
- Removing an airway retains both endpoint waypoints and reconnects them directly.
  An endpoint remains pinned if another airway still references it.
- The final exit supports Select Airway. Adding another airway shares that existing
  occurrence. A nonmatching selected airway entry/exit gets its own explicit waypoint,
  with a direct connecting leg to the selected plan anchor.
- Intermediate children offer navigation actions, not trimming or structural edits.
- Remove All Above is validated as a whole edit. Removing the inbound airway with its
  prefix may leave a valid exit; removing the shared exit of an outbound airway cannot.
- Existing SID/STAR/approach attachment rules still apply, including atomic removal of
  an airport and its attached procedures.

Text entry and menu selection use the same airway insertion implementation. Stored
geometry must be continuous and connect the referenced endpoints. Missing geometry is
an error, never a synthesized straight-line airway. NAVDB rebuild resolves the endpoint
occurrences and preserves their IDs. Projection emits only interior airway children;
overlap suppression remains solely for procedure boundaries.

## Compatibility

Cloud flight-plan records now require schema 3. Earlier schemas and the old inline-NavRef
airway representation are rejected, not migrated or repaired during projection. Existing
saved airway plans must be recreated. This does not change the NAVDB product contract.
The subsequent core-owned picker changes the UI wire contract to version 12.

## Regression Coverage

`planning/airway_tests.rs` checks 192 route configurations with zero, one, or two airways,
shared/separate junctions, prefixes/suffixes, and zero through two interior fixes. It
checks every projected action against an independent structural expectation and exercises
core action dispatch/mutation, including rejected actions, in inactive, active-leg, and
direct-to states. Additional tests cover repeated identifiers, deletion/unpinning,
serialization, and sequencing with agreement between map geometry, rows, and totals.

The in-memory NAVDB test in `had_ops.rs` exercises actual text parsing and airway-menu
materialization in both travel directions, then rebuilds against NAVDB and checks identity
and rendered geometry. Procedure-attachment tests cover combined SID/airways/STAR/approach
plans. Cloud tests reject old schemas and dangling endpoint references.

These tests require no published artifacts, browser, or emulator. The original explicit
endpoint and no-trimming regressions were verified failing before the model change.

## Core-Owned Airway Picker

The flight-plan controller owns the open picker, selected airway/entry, Back/Close,
labels, enabled reasons, and opaque click registrations. Its view is part of the existing
flight-plan incremental update, not a separately queried platform model.

The first section contains every airway whose spatial index includes the selected
canonical NavRef. The second selects nearby airways by nearest indexed waypoint distance,
targeting ten but including the entire tie at the tenth airway's distance. For KRNT,
all twenty routes sharing SEA survive that cutoff. Search uses the existing spatial
NAVKV index, expanding through 25/50/100/200/400 nm windows without rereading tiles.
Platforms neither supply a limit nor interpret membership or branch identities.

Both sections use one `AirwayMenuOrder`: low-altitude T/V routes first, high-altitude
J/Q routes second, other families last; within a group, prefix then numeric route number.
The preferred altitude band is an explicit policy parameter, with both directions tested,
so a future core setting can reverse the groups without changing selection or platform code.
Proximity decides membership; display ordering never excludes a nearer route.

An exact airway fixes the selected waypoint as entry when its branch occurrence is
unambiguous; other choices retain an entry step. Exit selection uses the same core
materialization and transactional insertion as text entry. Disabled entries have no
registered exit action. Generation-scoped click IDs reject old menus.

Opening and advancing do all NAVKV reads before committing a transition. Resource faults
therefore leave the previous picker intact. Definition edits close it; navigation updates
do not. NAVDB epoch changes invalidate it. Session rollback restores it with the rest of
the flight-plan model.

Web and Android render the sections using their existing dense buttons and standard
thumb gutter. Android submits row mutations and picker clicks to the existing serial
background mutation runner; the raw native methods require explicit scheduled-work
opt-in. Core owns the policy; this runner only executes and delivers results.

Behavior tests cover membership, sorting, limits, coincident-but-different NavRefs,
entry/exit/back, stale clicks, missing pages, definition/navigation changes, and the
immediate incremental update and final insertion through the session command API.
