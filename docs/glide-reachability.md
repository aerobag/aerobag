<!-- SPDX-FileCopyrightText: 2026 Aerobag contributors -->
<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Glide reachability

Enable **Glide reach** in the Chart layer menu. It uses the flight plan's
aircraft selection, including the existing default/model supersession policy,
and the Altitude Planner's No Wind or forecast selection. There is no separate
glide wind selector. The label shows the wind at the aircraft and the model's
best-glide IAS. With a known aircraft track, wind uses the Altitude Planner's
flow arrow and signed along-track component. The arrow indicates flow toward a
true bearing and rotates with the live map in TRK mode; the component and IAS
stay upright. Both use stroked text without a background panel.

## Calculation and scheduling

Core casts 180 straight ground-track rays, spaced two degrees apart. Each ray
advances in 0.025 nm steps, samples terrain, and stops at its first terrain
intersection. It cannot bend around a ridge or resume in the valley beyond it.
The last verified clear point is the endpoint. Terrain sampling uses the
maximum of neighboring cells; a missing center cell remains unknown. Unknown
directions leave gaps in the outline instead of assuming sea level.

At each step, the atmosphere supplies wind and temperature at the descending
position, pressure altitude, and time. IAS is converted to TAS, L/D resolves
horizontal speed and sink, and the wind triangle resolves ground speed along
the ray. The pressure-to-MSL altitude offset is held constant during descent.
Wind stronger than the available horizontal airspeed can stop progress in a
direction. These are estimates at a fixed best-glide IAS, not a speed polar or
a wind-optimized speed-to-fly calculation. There is no turn penalty, landing
reserve, obstacle database, or landing-site suitability calculation.

The shared background work scheduler runs eight bearings per batch on the web
core worker or Android background dispatcher. It yields between batches so
input can take priority. Resource requests use the existing forecast/terrain
fetch and cache machinery. A pending computation keeps a fixed origin; the
last completed result remains at its original geographic coordinates while a
replacement computes. Web uses `MapGeometryLayer`; Android uses the live
`MapDisplayFrame`, so pans and rotations do not wait for recalculation.

Recompute on any of:

- Movement of 0.5 nm, reduced to 10% of estimated still-air range when low
  (minimum 0.05 nm), measured from the last calculation's actual origin.
- MSL or pressure-altitude change of 5% of starting AGL, bounded to 25–100 ft.
- Thirty seconds of age, or a backwards clock change.
- Changed aircraft, wind selection, forecast version, NAV generation, or packages.

Unavailable forecast coverage restarts the entire calculation in no wind and
posts a GLIDE status message; it never mixes wind assumptions within one ring.
Missing aircraft performance, position, or MSL altitude suppresses the ring and
posts a status message. Incomplete terrain coverage produces partial paths and
a status message. Work is bounded to 200 nm and two hours per ray; hitting a
bound also leaves a gap.

## Aircraft data

Aircraft schema 3 adds optional glide data with IAS, estimated L/D, reference
weight, and source/configuration notes. Speeds are fixed at the recorded
reference weight; there is currently no actual-weight correction. Definitions
without glide data remain usable for other planning features.

| Bundled model | IAS | L/D | Basis |
| --- | ---: | ---: | --- |
| Generic C172 | 68 kt | 9 | C172S maximum-glide chart used as a generic proxy |
| 1975 C177RG | 73.9 kt | 9.7 | 85 mph at 2,800 lb; maximum-glide chart |
| PA46-310P | 90 kt | 14 | Approximate maximum-glide chart slope, propeller full decrease |
| Sinus | 54 kt | 23 | MAX 600 kg operating instructions, long tips, feathered propeller, flaps 0 |
| Virus | 62 kt | 20 | Same MAX 600 kg instructions, FLEX short-tip supplement, feathered propeller, flaps 0 |

Primary aircraft sources (the same assumptions are recorded in each model):

- [Cessna 172S POH](https://www.northcoastair.com/documents/Cessna%20172S%20POH.pdf), maximum glide.
- [1975 Cessna 177RG POH](https://cardinalflyers.com/mdata/poh/1975-177rg-poh.pdf), figure 6-6.
- [Piper PA46-310P POH](https://www.aeroelectric.com/Reference_Docs/Piper/PiperMalibu_PA46-310_SN4608008-140.pdf), figure 5-29.
- [Pipistrel SINUS 912 MAX operating instructions, AOI-119-00-40-001 B01](https://www.dropbox.com/scl/fi/ukn0actsvxg6yfmdukc2b/AOI-119-00-40-001_B01-Sinus-912-MAX-Aircraft-Operating-Instructions.pdf?rlkey=ud87gqm0ol10v18pszmc9qbdf&dl=0),
  issued April 17, 2024, available from [Lanier Flight Center's manual library](https://www.lanierflightcenter.com/lanier-flight-training/student-resources/).
  Long tips: sections 3.4.2 and 5.9.2, printed pages 3-9 and 5-13
  (PDF pages 51 and 129). Short tips: FLEX supplement sections 3.4.2 and 5.9.2,
  printed pages 9-S1-5 and 9-S1-14 (PDF pages 205 and 214).

Both Pipistrel models use that same manual's published 600 kg / 1,320 lb
figures, with airbrakes retracted. The Virus model represents the MAX FLEX
short-tip configuration, consistent with its existing powered-flight profiles.
Its 62 KIAS is a published value, with no weight scaling. The manual covers
standard tailwheel and optional nosewheel configurations but supplies no
separate nosewheel glide correction; none is invented here.

The earlier 23:1/24:1 pair mixed current MAX data with a 550 kg FLEX manual.
The MAX manual resolves this to 23:1/20:1. Retired demo model hashes remain in
`supersedes` so an existing model selection can resolve to the correction.
The fixed gross-weight reference is a modeling choice, not a guaranteed
conservative bound on wind-adjusted ground range.

NAV28 publishes the new bundled definitions and their content hashes. Their
`supersedes` lists retain the previous hashes. Frozen schema-2 definitions still
decode and hash identically; encrypted cloud roots are not migrated by this
feature. A custom older model needs glide data before it can produce a ring.

## Verification and publication

Core tests cover analytical still-air range, head/tail/crosswind, changing wind
through descent, a blocking ridge, unknown terrain, invalid inputs, refresh
thresholds, and incremental versus whole calculations. Session tests cover
paging/batches, cached results, wind selection, and disabling the layer. Web
tests cover geographic ownership and asynchronous disposal. Android's physical
touch test uses the production map layer owner while panning, zooming, and
rotating, and verifies both map and Home receive taps through the overlay.

NAV28 extends NAV27's NOTAM-catalog schema 2 with the schema-3 bundled aircraft
models. NAV27's immutable descriptor remains unchanged. The pinned Android smoke
and release-journey fixtures were rebuilt from the normal NAV28 publication;
the release fixture also includes the current contract-9 NOTAM feed. All five
bundled aircraft definitions were read through the shared NAV reader and matched
against the checked-in models. The N550AR browser replay renders the glide ring
using that normal publication.
