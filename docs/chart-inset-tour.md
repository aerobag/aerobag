# Chart Inset Tour

Tour of the September 2026 curation: 14 published manually georeferenced insets,
one exclude-only region, and eight FAA-georeferenced detail sheets.
Juneau covers three entries at one stop.
The current metadata must be included in a cycle build before this tour can
validate the latest edits; restarting the app alone does not rebuild chart tiles.

Paste into the flight-plan free-text field:

```text
KMWC KIND KRRT KJAX KTPA KMTH L41 KSEA CYVR PAKT PAJN 60.1920,-139.9879 PANC PAFA PAOM 65.0688,-172.0758 PADQ PADU PASN KILM PGUM
```

Airport identifiers were checked against the published NAV25 database on
2026-09-12, and their positions checked against the curated inset boundaries.
Two coordinate waypoints put the view directly inside regions without a useful
airport in that database. Both use the normal flight-plan coordinate syntax.

| Stop | What to inspect | Layer |
| --- | --- | --- |
| KMWC | Milwaukee inset, from Chicago SEC | TAC |
| KIND | Indianapolis inset, from St Louis SEC | TAC |
| KRRT | Lake of the Woods / Northwest Angle inset, from Twin Cities SEC; pan north from Warroad | TAC |
| KJAX | Jacksonville inset, from Jacksonville SEC | TAC |
| KTPA | Native FAA Tampa TAC; redundant Jacksonville SEC Tampa inset is excluded, not overlaid | TAC |
| KMTH | Florida Keys inset, from Miami TAC; follow the Keys west toward Key West | TAC |
| L41 | Marble Canyon inset, from Grand Canyon General Aviation | TAC |
| KSEA | FAA Seattle detail, ENR_AKH01_SEA | IFR-H |
| CYVR | FAA Vancouver detail, ENR_AKL01_VR | IFR-L |
| PAKT | Ketchikan inset, from Ketchikan SEC | TAC |
| PAJN | Juneau inset, from Juneau SEC | TAC |
| PAJN | Juneau High Density Traffic Area, from Juneau SEC | Flyway |
| PAJN | FAA Juneau detail, ENR_AKL01_JNU | IFR-L |
| 60.1920,-139.9879 | Seward Glacier Area, from Juneau SEC | TAC |
| PANC | FAA Anchorage detail, ENR_AKL04_ANC | IFR-L |
| PAFA | FAA Fairbanks detail, ENR_AKL03_FAI | IFR-L |
| PAOM | FAA Nome detail, ENR_AKL03_OME | IFR-L |
| 65.0688,-172.0758 | Lavrentiya / Provideniya, Russia inset, from Nome SEC | TAC |
| PADQ | Kodiak inset, from Kodiak SEC | TAC |
| PADU | Dutch Harbor / Unalaska inset, from Dutch Harbor SEC | TAC |
| PASN | Pribilof Islands inset, from Dutch Harbor SEC; includes St Paul and St George | TAC |
| KILM | FAA Wilmington detail, ENR_L23_WILM_INSET | IFR-L |
| PGUM | FAA Guam detail, ENR_P01_GUA | IFR-H |

Enable the indicated raster layer and zoom in enough to load its detail.
At PAJN, inspect TAC, Flyway, and IFR-L separately. Reference-only diagrams,
including the New Orleans Special Air Traffic Rule extraction, are Chart
References on the Plates page, not georeferenced map overlays.

This is a visual QA itinerary, not an operational flight plan.
