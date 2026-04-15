# Performance Optimization

## Fast-start experiment

Branch: `jonh/perf-optimization`

Purpose:
- Measure how much startup time we can recover by rendering the chart/plate shell first and loading the wasm adapter, sqlite wasm, nav DB, and UI session in the background.

Observed result:
- The experiment was "remarkably usable".
- Roughly `~3.6s` to first visible page content.
- The `PLN` page populated shortly after background initialization completed.

Compared with the previous startup shape, this experiment confirms that a large part of perceived startup delay is not required for first chart/plate paint.

## What changed in the experiment

The web app was changed in a deliberately crude way:

- `App.tsx` no longer blocks all rendering on adapter/session readiness.
- Adapter loading was moved behind a post-paint lazy boundary using dynamic `import("./domain/appCoreAdapter")`.
- The initial UI renders from static sample chart/map data immediately.
- The real wasm adapter, sqlite wasm, nav DB open, and session creation happen in the background.
- The startup shell still dismisses on first real visual content, not on core readiness.
- The hidden `PLAN` page was prevented from crashing the app during background bootstrap by only mounting `FlightPlanPage` once `planUiState` exists.

This is intentionally not production quality. The goal was measurement, not correctness under all intermediate states.

## What we learned

1. Deferring heavy startup work is worth real user-visible time.
   The app can become usable well before nav DB open and session creation finish.

2. `sqlite-wasm` and adapter/session startup are meaningful contributors.
   Even with compressed nav DB transfer, startup still spends substantial time in:
   - loading/evaluating sqlite wasm
   - opening the nav DB
   - creating the UI session and first snapshot

3. Chart/plate shell rendering is much less dependent on core than the app currently assumes.
   A visible page can come up first, while deeper plan/guidance functionality hydrates later.

4. Hidden pages matter.
   During the experiment, the hidden `PLAN` page crashed the app until it was prevented from mounting without `planUiState`.
   Any future staged-startup design needs to treat off-screen pages as real runtime participants.

## Follow-up if revisited

If this experiment is revived, the next step should be to make the fast path intentional instead of accidental:

- define which pages/features are allowed before session readiness
- keep non-ready pages/components from mounting unstable hook trees
- add explicit "session warming" / "nav DB warming" states instead of relying on null-tolerance
- separately instrument:
  - seeded plan build
  - `createUiSession`
  - first snapshot
  - ownship seed

This branch should be treated as a reference spike, not as a directly shippable implementation.
