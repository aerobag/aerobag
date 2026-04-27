# Web Persistence Plan

## Problem

On Android Chrome, after the user backgrounds the app for long enough or the device comes under memory pressure, the browser may discard the live page/JS/WASM process.

When the user returns, they can briefly see the last composited frame, then the page restarts:

- splash screen reappears
- viewport resets
- map/layer/page configuration resets
- a fresh core session is created

This is not primarily a Vite-dev quirk. A production build would still suffer the same class of interruption as long as the app's real state lives only in volatile JS/WASM memory.

## Current Behavior

Today the web app persists only a small slice of UI state in `localStorage`:

- `page`
- `selectedAirportId`
- `selectedChartId`
- `recentAirportIds`

The app also keeps richer transient navigation state in `window.history.state`, including `mapViewport`, but that only survives while the page process and history entry remain alive. It does not help after Android discards the tab process.

On startup, the app currently:

1. initializes the adapter
2. creates a fresh core session
3. builds the default seeded dev flight plan
4. initializes the default viewport

So after a mobile tab discard, the user gets a cold restart, not a restored session.

## Goal

Returning to the app after backgrounding should restore the user's working context instead of restarting from defaults.

That means we need restart-safe persistence for actual app/session state, not just a few UI crumbs.

## Architectural Direction

Core should own a serializable session snapshot.

Web should:

1. request that snapshot from core on meaningful state changes
2. persist it locally
3. restore from it on startup instead of always creating a fresh default session

This keeps business/state semantics in core and keeps web responsible only for storage and bootstrapping.

## Minimum Snapshot Contents

The persisted snapshot should include enough state to resume without surprising the user.

At minimum:

- current page
- map viewport
- selected map id / map-family selection
- map layer visibility state
- flight plan
- guidance / active leg
- chart page selection state
  - selected airport
  - selected chart
  - recent airport ids

Likely also worth including:

- chart viewport
- chart folder open/closed state
- ownship source selection state
- replay state if replay continuity is desired

## Restore Strategy

Replace the current startup assumption of "always create the seeded default session" with:

1. load persisted snapshot from storage
2. if snapshot exists and is compatible:
   - ask core to restore a session from it
3. otherwise:
   - create the current default seeded session

If restore fails:

- log the failure
- discard the bad snapshot
- fall back to fresh session creation

The app should fail soft, not brick startup.

## Storage Choice

Use either:

- `localStorage` for the first cut if the snapshot stays small enough
- `IndexedDB` if snapshot size or write frequency makes `localStorage` clumsy

Bias:

- first cut can use `localStorage` if the snapshot is compact JSON
- move to `IndexedDB` if replay/session/map state grows large or write batching becomes necessary

## Persistence Triggers

Persist on meaningful state changes, not every render.

Candidate triggers:

- session snapshot changes
- map viewport changes
- page changes
- chart selection changes
- layer visibility changes
- flight plan mutations

Also persist on lifecycle hints when available:

- `visibilitychange`
- `pagehide`

These lifecycle hooks are best-effort only. They are not sufficient by themselves, so regular incremental persistence is still required.

## Scope Boundaries

This plan is about restart continuity after mobile browser discard.

It is not primarily about:

- offline/PWA installation
- service workers
- suppressing the splash screen
- eliminating startup latency

Those may matter separately, but they do not solve the underlying continuity gap.

## Recommended Implementation Order

1. Define a core-owned serializable web session snapshot.
2. Add a core restore entrypoint that rebuilds a session from that snapshot.
3. Replace the current ad hoc web `localStorage` crumbs with persisted core session state plus a small amount of web-only view state if still necessary.
4. Restore viewport/map/page/layer state on startup before showing the app as ready.
5. Add lifecycle-triggered flushes as a reliability improvement.

## Expected Outcome

After this work:

- Android Chrome tab discard should no longer feel like a hard app reset
- users should return to the same flight plan, viewport, selected page, and map/layer configuration
- web and core responsibilities remain clean:
  - core owns session semantics
  - web owns persistence transport/storage
