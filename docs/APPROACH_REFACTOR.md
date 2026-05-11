# Approach Handoff Refactor Plan

Goal:
- replace the growing set of pairwise leg-handoff fixes with one richer handoff contract and one reconciler
- preserve or improve current nationwide approach behavior while making the validator stricter about geometric nonsense

Non-goal:
- do not regress into airport-named geometry hacks when a general handoff interpretation can explain the case

## Flight Plan Leg Vocabulary

Procedure geometry introduces three distinct route granularities. Keep these names
stable in code, docs, and UI discussions:

- `RouteComponent`: the user-authored flight-plan unit, such as an airport,
  VOR, airway, whole procedure, or next airport. Components are the editing and
  replacement boundary.
- `GuidanceLeg`: a CDI/sequencing unit, usually bounded by named waypoints
  inside a route component. A procedure `RouteComponent` expands into multiple
  guidance legs so step-down fixes can sequence and each visible guidance leg
  can be activated independently.
- `PathElement`: a drawable/flyable geometry primitive inside a guidance leg,
  such as a straight segment or arc. Path elements are what map rendering uses
  now and what future ownship advancement should track at fine granularity.

Relationship:

```text
RouteComponent -> GuidanceLeg(s) -> PathElement(s)
```

UI rule:
- `Activate Leg` on a waypoint row activates the `GuidanceLeg` that ends at that
  row's waypoint. The button means "give me guidance to this destination." For
  example, inside `KPAE VOR-A/ECEPO`, the `YAVUR` row activates
  `ECEPO -> YAVUR`, and the route renderer paints that guidance leg's path
  elements as active.

## Why This Refactor Exists

The current implementation still leaks handoff responsibility across many pairs:
- `DF -> CF`
- feeder `CF -> common CF`
- `PI -> feeder CF`
- `HF/HM -> IF/CF`
- kinked common `CF -> CF`

That is not because the spec is inherently quadratic. It is because our current handoff contract is too weak, so every boundary has to guess whether:
- the inbound leg already satisfied part of the outbound leg
- the outbound leg's coded fix is stale or behind us
- we should resume, rejoin, skip, or yield

The refactor should move that guesswork into one explicit contract and one reconciler.

## Phase 0: Freeze The Current State First

This phase goes first.

Reason:
- we need one git commit that captures the current nationwide decoding as data before the refactor starts
- later, if we realize we wanted more fields in the capture, we can branch from that commit, improve the capturer, and rerun it without mixing in refactor changes

Deliverables:
1. a commit whose only job is to capture current nationwide approach behavior
2. snapshot artifacts written somewhere stable and obvious
3. a documented command for reproducing the capture

Current capture command:

```bash
env AEROBAG_FIXTURE_NAV_DB_PACKAGE=/root/aerobag-artifacts-snapshot/published_packaged/nav_db_2604_01_19f2219ad5064fab7ea983c654a031ee31452a7e649ed677b5518a4283fc4059.zip \
  cargo test -p app-core capture_all_snapshot_approaches_with_progress_logging -- --ignored --nocapture
```

Current output paths:
- `/tmp/aerobag-approach-capture-progress.log`
- `/tmp/aerobag-approach-captures.jsonl`
- `/tmp/aerobag-approach-capture-status.txt`
- `/tmp/aerobag-approach-capture-summary.json`

Recommended captured data:
- one record per `(airport_id, procedure_id, enroute_transition)`
- success or failure
- failure message if any
- resolved-leg inventory:
  - `from`
  - `to`
  - procedure provenance
  - path termination
  - leg sequence
- display-path inventory:
  - element list
  - element geometry
  - effective terminal course
  - debug source labels if available
- heading-signature inventory:
  - start/end labels
  - drawn start/end course
  - logical terminal course if different
  - element kind
- summary counts:
  - total cases
  - successes
  - failures

Preferred storage shape:
- one manifest JSON
- one JSONL stream for per-case data
- optional compact text dump for quick grepping

Suggested file location:
- a dedicated `/tmp` capture for local iteration
- plus a durable repo-adjacent or ignored artifact directory for the frozen baseline

Acceptance criteria:
- we can diff later runs against this baseline mechanically
- the baseline commit is cleanly identifiable in git history

## Phase 1: Define The Handoff Contract

Add explicit types for:
- `TerminalState`
- `StartRequirement`
- `HandoffDecision`

### TerminalState

This is what an inbound leg publishes after it has been interpreted.

It should contain:
- `terminal_position`
- `drawn_terminal_course_deg`
- `logical_terminal_course_deg`
- `terminal_anchor`
  - optional fix/navaid identity if terminated at an anchor
- `established_course`
  - optional course/radial descriptor if the leg ended established on one
- `incoming_course_to_anchor`
  - optional course if we arrived at an anchor via a known inbound leg
- `outgoing_course_from_anchor`
  - optional course if the leg completed onto a known outbound leg
- `hold_state`
  - none / entered / completed outbound / completed inbound / established inbound
- `procedure_turn_state`
  - none / entered / completed / established on following course
- `common_segment_state`
  - not-on-common / at-common-anchor / established-on-common-course / resumed-through-kink
- `coded_fix_satisfaction`
  - whether the leg's coded fix is still ahead, exactly satisfied, or behind/stale

Important design rule:
- `drawn_terminal_course_deg` and `logical_terminal_course_deg` must remain distinct
- validation and reconciliation need both

### StartRequirement

This is what an outbound leg asks for.

It should be representable without naming the inbound leg type.

Examples:
- `AtFix(anchor)`
- `DirectToFix(anchor)`
- `EstablishedOnCourse(course_descriptor)`
- `InterceptCourse(course_descriptor)`
- `ResumeCommonSegment(common_segment_descriptor)`
- `EnterHold(hold_descriptor)`
- `ContinueClimbOnCourse(course_descriptor, altitude_constraint)`

### HandoffDecision

This is what the reconciler returns.

Initial shape:
- `ContinueAsDrawn`
- `ResumeAtAnchor`
- `ResumeThroughAnchorKink`
- `SkipStaleFix`
- `YieldToFollowingCourse`
- `BuildJoinGeometry`
- `EnterHold`
- `Invalid`

## Phase 2: Build One Reconciler

Add one reconciler that consumes:
- `TerminalState`
- `StartRequirement`
- nearby procedure context

and produces:
- `HandoffDecision`

This should live above low-level geometry generation.

Responsibilities:
- detect when a coded fix is already satisfied
- detect when a coded fix is behind us
- detect when we are already on the common segment
- detect when we are at a common anchor but the common segment kinks there
- decide whether the next leg should own geometry or yield

Non-responsibilities:
- it should not directly emit arcs or segments
- it should not know airport names

## Phase 3: Migrate Existing Heuristics Into The Reconciler

Do this incrementally.

Migration order:
1. stale `DF -> following CF`
2. feeder `CF -> common CF`
3. `PI -> feeder CF -> common`
4. `HF/HM -> IF/CF`
5. common-segment resume through anchor kinks

For each migration:
1. express the existing pairwise rule in terms of `TerminalState` + `StartRequirement`
2. prove the reconciler can return the same decision
3. delete the old ad hoc rule
4. rerun the nationwide diff + validator suite

Success criterion:
- the list of pair-specific predicates in `lib.rs` should shrink toward zero

## Phase 4: Keep Builders Dumb

Low-level geometry builders in `procedure_geometry.rs` should:
- interpret one leg/window
- publish a rich `TerminalState`
- obey a `HandoffDecision`

They should not:
- invent their own resume policy
- decide whether a fix is stale
- skip a common segment on their own

That policy belongs in the reconciler.

## Phase 5: Strengthen Validation

We should validate both:
- logical continuity
- drawn continuity

Current lesson:
- logical continuity alone let bad hairpins survive

### 5a. Preserve Existing Checks

Keep:
- path-gap validation
- logical heading continuity
- existing special-case published acute-turn allow-lists

### 5b. Add A Drawn-Boundary Check

At every boundary, compare:
- inbound drawn terminal tangent
- outbound drawn start tangent

This should fail on:
- instantaneous hairpins
- hidden 180 flips masked by logical terminal course

This check already proved useful in the current work.

### 5c. Tighten Default Tolerances

Target default:
- `10°` almost everywhere

Allow larger turns only for named reasons:
- hold entry / hold completion
- procedure turn completion
- course intercept / capture geometry
- arc-to-course or course-to-arc transitions
- explicit charted acute waypoint turns

## Phase 6: Make Large-Turn Allowances Explicit

Do not keep broad anonymous `120°` buckets unless they correspond to a real category.

Preferred shape:
- `allow_hold_entry_turn(...)`
- `allow_procedure_turn_exit(...)`
- `allow_course_capture_turn(...)`
- `allow_charted_acute_waypoint_turn(...)`
- `allow_charted_exception_ksan_pgy(...)`

For acute waypoint turns, likely generic inputs are:
- both inbound and outbound are fix-defined
- both legs are long enough to allow anticipation
- turn occurs at the shared waypoint
- no immediate reversal/backtrack through the waypoint

This phase is where we decide which current explicit exceptions remain justified.

## Phase 7: Nationwide Regression Discipline

Every refactor stage must run:
1. current materialization tests
2. full nationwide audit
3. diff against the phase-0 baseline

We should expect diffs in three classes:
- unchanged
- changed for the better
- suspicious/regressed

We need tools that summarize:
- count of changed cases
- count of changed geometries
- count of changed failures
- exemplars with links to text/PNG outputs

This is the main safety net for the refactor.

## Phase 8: Concrete Working Order

Recommended order of work:

1. Freeze current nationwide behavior as data.
2. Add `TerminalState`, `StartRequirement`, `HandoffDecision` types.
3. Teach builders to publish richer terminal state, with no behavior changes yet.
4. Add the reconciler alongside old logic.
5. Port one heuristic family at a time into the reconciler.
6. Add the drawn-boundary validator as a separate check.
7. Tighten default tolerance to `10°`, then add named wideners back only where justified.
8. Delete obsolete pairwise rules.

## Phase 9: Exit Criteria

We are done when:
- pairwise handoff logic is centralized or nearly centralized
- nationwide diff against the frozen baseline is mostly unchanged or demonstrably improved
- drawn-boundary validation catches instantaneous flips
- remaining widened-turn allowances are explicit and justified
- approach rendering no longer depends on a growing list of special pair fixes

## Immediate Next Step

Do phase 0 first:
- commit the current behavior capture as data
- document the exact capture command and output paths
- only then start changing the handoff contract
