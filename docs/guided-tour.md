<!-- SPDX-FileCopyrightText: 2026 Aerobag contributors -->
<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Guided tour

A fresh installation automatically opens the tour after disclaimer acceptance.
Core persists a separate local `tour-introduction-v1` marker as soon as it is
offered; closing, finishing, restarting and Cloud Sync cannot reset it. Existing
core settings without that marker count as an existing user. The automatic
introduction returns to Home when closed, ready for work or Cloud pairing.

Home opens the tour or resumes the step last closed in this app session.
Start over returns to the first step; Finish clears that bookmark. Next and
Back select deterministic scenes; Close and Finish
restore the user's plan (including undo history and route drafts), preferences,
ownship selection, plate selection, navigation history and viewport. Demo edits
are neither persisted nor sent to Cloud Sync. Live resources and receiver data
continue to advance. An app restart loads the user's persisted state.

Page presentation has a separate lifetime for normal use and for the tour.
Web's shared `PageLayer` and Android's `GuidedTourHost` recreate page-local UI
when core starts or ends the tour. This disposes demo menus, dialogs, portals,
and page effects even when the restored page is the same page, or a web page
is retained while hidden. New pages inherit this boundary; they must not add
per-tray Close-tour cleanup lists. Step changes keep the same presentation
lifetime. The session and saved view remain above this boundary; transient
page controls reopen closed when returning to normal use.

The script, platform eligibility, scene state and actions live in
`ui/core-rust/crates/app-core/src/guided_tour.rs` and
`guided_tour_session.rs`. Demo routes use the ordinary airway picker and router,
with the installed/published navigation data. Altitudes and weather are real
results, not fixed screenshots or invented values. Missing data is reported by
the existing resource boundary; the user can close the tour.

The generated `UiGuidedTour` contract gives clients a page, surface, subject,
selected row UID, viewport, optional geographic inspection point, card placement, title-card presentation and semantic
target names. Chapter cards use the ordinary card size and center their title without body text.
Following cards inherit their chapter's title as the gray supertitle. The SPOT
scene frames Enumclaw separately from the inspected point to leave room for the
inspector. Its map arrow measures the marker drawn in the shared map frame,
alongside arrows to the SPOT button and +INSERT.
Core also supplies the keyboard shortcuts: Enter or N advances; B goes back.
They follow the buttons' enabled state and ignore held-key repeats.
Arrows use orange strokes outlined in white and dark brown for contrast against
charts and panels. Start over sits above Close, away from Next. Platform adapters locate
the actual controls and open their ordinary trays. They must not implement a
second tour script or decide which waypoints/airways to use. Next waits for the
scene's target controls to appear. Scene errors belong to their generation.

Web renders `GuidedTourOverlay` through a body portal. Android renders
`GuidedTourHost` above an app plane and registers control bounds through the
shared indexed-control modifiers. Android popups must use `TourAwarePopup`:
it retains native Popup behavior normally and moves Compose content into the app
plane during the tour, so menus and inspectors cannot bypass the transparent
input blocker or lose asynchronous content updates.

The offline demonstration invokes `guided_tour_packages_preview` against a copy
of the catalog/controller. Its only output is UI; it cannot expose persistence,
cloud preference updates, downloads or deletion commands. A fresh installation
may fetch catalog metadata for that copy without installing any products.

Verification includes core restoration and wire-envelope tests, receiver pause
lifecycle tests, physical Android overlay taps in both orientations and a real
unmount/remount test. `shared.guided-tour` exercises Next, Back, a menu, Close,
plan restoration and reopening on both clients. The full narrated route uses
current data for KRNT, KUAO, KMWC, SEA, BRUKK and OCS; the compact journey is not
coverage of every chapter or the live data used by those examples.
