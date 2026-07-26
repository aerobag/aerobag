# Android NEXRAD Live Feed Follow-Ups

Web now renders NEXRAD through the live-feed source-grid path. Android native now
uses the same core-provided source-grid overlay geometry. Android intentionally
uses a different acquisition policy: it eagerly downloads and durably retains
complete recent NEXRAD frame packages so animation continues through network
loss. Web fetches selected tiles just in time. Both policies feed the same
core-owned frame catalog and animation model; see
`docs/refactor/nexrad-acquisition-and-animation.md`.

Remaining follow-ups:

- Android policy is currently a conservative first pass: live-feed downloads are
  allowed on local/dev endpoints and on unmetered networks. A user-visible
  setting can relax or tighten that later.
- Android installs/caches winds-aloft packages, but there is still no visible
  winds-aloft map consumer.
- Android debug UI has the `NEXRAD tile labels` flag in the shared debug model,
  but tile-label rendering remains web-only.

Platform code should only provide fetch/SSE mechanisms, package persistence, and
bitmap drawing. Core owns the live-feed contract, selected state, NEXRAD
retention, resolution choice, animation, typed resource source, and overlay
geometry.
