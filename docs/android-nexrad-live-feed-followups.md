# Android NEXRAD Live Feed Follow-Ups

Web now renders NEXRAD through the live-feed source-grid path. Android native is
intentionally not being chased in this pass, but it is behind in these ways:

- Android still has the older frame-oriented NEXRAD path (`nexradFrameBytes`,
  frame list loading, and frame-index animation).
- Android does not yet consume the live-feed SSE/current/version/state flow for
  NEXRAD.
- Android does not yet render core-provided source-grid NEXRAD overlay pieces.
- Android debug UI has the `NEXRAD tile labels` flag in the shared debug model,
  but the live source-grid label/rendering behavior is currently web-only.

When Android catches up, platform code should only provide fetch/SSE mechanisms
and bitmap drawing. Core should continue to own the live-feed contract, selected
state, NEXRAD resolution choice, and overlay geometry.
