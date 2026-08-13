---
id: TASK-79
title: NavCanada
state: someday
assignee: []
created_date: '2026-05-12 19:39'
labels:
  - cat:data
dependencies: []
ordinal: 79000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Be nice to proof-of-concept loading NAV CANADA data, if it is possible.

Research notes (2026-08-05):

The free NAV CANADA Designated Airspace Handbook (DAH) is useful but incomplete. It contains coordinates for intersections/fixes and low-level airway/RNAV route sequences, but explicitly omits fixes used only for ATC purposes. Airport and NAVAID entries use friendly names rather than consistently supplying the ICAO airport ident or transmitted NAVAID ident. For example, CYLW appears indirectly as `Kelowna, BC - AD`; joining it to `CYLW` requires another catalog. The CFS reportedly has the complete alphabetical waypoint list, while the comprehensive machine-readable Data Pack D/B is licensed/quote-based. We still appear unable to obtain the enroute charts as convenient public digital data.

Potential community catalogs:

| Property | OurAirports | OpenAIP |
| --- | --- | --- |
| Airports, runways, frequencies | Strong | Strong |
| NAVAIDs and transmitted idents | Strong | Available |
| IFR fixes/reporting points | None | Has reporting points, but Canadian IFR completeness is unproven |
| Airway/route topology | None | No evident airway graph export |
| Airspace and obstacles | None | Available |
| Distribution | Simple nightly CSV downloads | API and daily exports, with more access friction |
| License | Public domain | CC BY-NC 4.0 |
| Authority | Community-maintained | Community-maintained |

OurAirports is the cleaner source for mapping DAH airport and NAVAID names/coordinates to real idents: its schema and licensing are straightforward. Its fatal limitation is that it has no intersections/fixes or airway topology.

OpenAIP covers more feature classes, including reporting points, airspace, and obstacles. However, its reporting-point category appears to include VFR reporting points and possibly some IFR fixes. Do not assume it contains Canada's complete five-letter IFR waypoint inventory without measuring it. Its published exports also do not appear to provide airway topology.

Proposed proof of concept:

1. Parse the official DAH for low-level airway fixes and route edges.
2. Join DAH airports and radio NAVAIDs to OurAirports by type, normalized name, and coordinate, failing closed on ambiguous or distant matches.
3. Audit every Canadian OpenAIP reporting point against the DAH fix inventory to measure actual IFR coverage. Use OpenAIP only as a cross-check or gap filler, with its attribution/license requirements handled explicitly.
4. Produce an audit report for unmatched, ambiguous, and conflicting records rather than guessing identifiers.
5. Decide whether the resulting partial low-level network is useful. Complete Canadian IFR coverage, including high-level, terminal, and ATC-only fixes, probably requires licensed NAV CANADA data.

References:

- NAV CANADA Aeronautical Information Products: https://www.navcanada.ca/en/aeronautical-information/aeronautical-information-products.aspx
- OurAirports data dictionary: https://ourairports.com/help/data-dictionary.html
- OurAirports downloads/license: https://ourairports.com/data/
- OpenAIP: https://www.openaip.net/
- OpenAIP export formats/object types: https://github.com/openAIP/openaip/issues/292
<!-- SECTION:DESCRIPTION:END -->
