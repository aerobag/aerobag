# Journey reliability rules

Read the root [agent rules](../../AGENTS.md) and
[hosted-test guide](../../docs/testing/hosted-ci.md). These rules apply to journeys,
shared drivers, and the platform code that supplies their readiness evidence.

## Diagnose the boundary that failed

- Preserve the first failure's trace, screenshot, hierarchy/projections, and
  app/fixture revisions. Distinguish a wrong expectation, premature readiness,
  lost or misdirected input, an application defect, and an observation failure.
  A timeout alone does not establish that CI was slow.
- Reproduce the ordering deliberately: hold and release a resource response,
  control a coroutine's completion, or unmount/remount the real component.
  Demonstrate that the regression fails with the old behavior and passes with
  the fix. Repeated successful reruns are not a causal explanation.
- Do not fix races with sleeps, longer deadlines, extra identical samples, or
  repeated actions. Prefer producer-owned readiness and framework events;
  keep necessary external waits bounded and diagnostic. Fail permanent errors
  immediately; transport failure must not masquerade as an absent control.

## Readiness belongs to the rendered resource

- A selected ID, launcher label, or retained viewport does not prove that its
  image and input geometry are ready. Publish readiness from the rendering
  owner, for the current resource, only when its handler can accept the action.
  Repeatedly observing the same premature state cannot establish readiness.
- Key both asynchronous work and its displayed value by resource identity and
  revision. Compose `produceState(keys)` restarts the producer but can retain
  the previous value. Reuse
  [produceKeyedResourceState](../../ui/android-app/app/src/main/java/org/aerobag/app/KeyedResourceState.kt)
  for that resource lifetime. On web, commit handler geometry and advertised
  readiness together at the layout boundary.
- Test replacement and actual unmount/remount with retained application state.
  Use production owners, not just mocked journey state. Examples:
  [web plate readiness](../../ui/web-app/src/plateReadiness.test.tsx) and
  [Compose resource lifetime](../../ui/android-app/app/src/test/java/org/aerobag/app/KeyedResourceStateTest.kt).

## Separate readiness, input delivery, and semantic completion

- Use [runtime transitions](release-journey-runtime.mjs): read readiness,
  perform one action with that evidence, then observe its specific result.
  Observation readers and acceptance predicates must remain read-only.
- Browser protocol acknowledgement does not prove browser event delivery;
  delivery does not prove application behavior. Reuse
  [browser input observation](browser-input-observation.mjs) for gestures: check
  live bounds and hit targets, subscribe before sending input, and await its
  final event. Preserve the receipt and still require a semantic postcondition.
- Use [gesturePlate](plate-gestures.mjs) for plate pan/zoom. It requires the same
  document and numeric movement in the requested direction. An unrelated
  session revision or arbitrary viewport-string change is insufficient.
- Return raw typed state from observations. Use `runtime.observe(description,
  read, accept)` or a transition's `completionSatisfied` predicate to decide
  success separately. [Transition failures](transition-contract.mjs) retain the
  last value and changed-state history; returning `null` for every mismatch
  discards the evidence needed to distinguish wrong state from missing state.
- Keep helper extraction inside the guardrails: add new journey helper modules
  to `AUDITED_JOURNEY_FILES` in the [structural audit](journey-structure-audit.mjs).
  Preserve its prohibitions on mutation in probes and unclassified timeouts.

## Put assertions at the layer that controls their inputs

- An unobscured map point is not necessarily empty geography. Ask the semantic
  driver for geometry, then explicitly select the desired inspector category.
  Do not choose magic coordinates to dodge fixture airports or terrain.
- Test automatic hit-test priority/fallback with controlled feature sets in
  [core](../../ui/core-rust/crates/app-core/src/map_overlay.rs)
  (`map_selection_returns_point_and_spot_categories`). Test real-platform SPOT
  selection and its terrain result in the journey. Name coverage for what it
  actually proves: `inspector.spot-selection` does not prove automatic fallback.
- Add cheap controlled regressions at the failing owner. Shared examples are
  [gesture contracts](plate-gestures.test.mjs) and
  [browser event observation](browser-input-observation.test.mjs). Model tests
  and source audits do not establish that real platform input/rendering works.
- Run each changed journey on every claimed platform with matching built app
  bytes. Report unrun checks explicitly. Follow the root cheap-preflight policy
  before committing; cheap preflight alone does not establish hosted E2E success.

These rules came from two flakes: wheel input authorized before a plate image
loaded, and a SPOT-fallback assertion at a map point that correctly selected a
nearby heliport. Investigation also exposed retained pixels during Android
resource replacement. Do not assume a flaky test means the product is correct.
