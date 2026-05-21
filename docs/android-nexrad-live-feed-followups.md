# Android NEXRAD Live Feed Follow-Ups

Web now renders NEXRAD through the live-feed source-grid path. Android native now
uses the same core-provided source-grid overlay geometry and reads tile bytes
from the installed whole-product live-feed package.

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
resolution choice, and overlay geometry.
