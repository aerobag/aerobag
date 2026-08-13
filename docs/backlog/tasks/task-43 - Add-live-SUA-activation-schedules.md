---
id: TASK-43
title: Add live SUA activation schedules
state: low
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - data
  - safety
  - feature
  - cat:productionization
dependencies: []
references:
  - https://sua.faa.gov/datafeed/suagw/n24sua?user=
  - https://sua.faa.gov/ops/docs/suagwDataFmt.html
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add live Special Use Airspace activation schedules from the FAA SUA gateway, including the `n24sua` feed for non-MTR reservations scheduled in the next 24 hours. Access may require contacting FAA support rather than self-service signup.


Notes left over from NOTAM adventure:

Earlier I asked you to produce an accounting/categorization of the various
NOTAMs we've collected so far. Can you do that again now?

   Category                     Distinct NOTAMs 
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━  ━━━━━━━━━━━━━━━━━                                                 
   Airspace                                 538                                                  
  ───────────────────────────  ─────────────────                                                 
   Obstacles                                518                                                  
  ───────────────────────────  ─────────────────                                                 
   Airport/movement area                    444                                                  
  ───────────────────────────  ─────────────────                                                 
   Navigation/comms/services                111                                                  
  ───────────────────────────  ─────────────────                                                 
   Procedures                                83                                                  
  ───────────────────────────  ─────────────────                                                 
   Total                                  1,694                                                  
   Airspace subtype                                 Count
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ━━━━━━━
   Special-use/restricted/warning/MOA activation      231
  ───────────────────────────────────────────────  ───────
   UAS/unmanned aircraft                              225
  ───────────────────────────────────────────────  ───────
   Other airspace notices                              25
  ───────────────────────────────────────────────  ───────
   Aerobatic activity                                  14
  ───────────────────────────────────────────────  ───────
   Explicit TFRs                                       11
  ───────────────────────────────────────────────  ───────
   Parachute jumping                                   10
  ───────────────────────────────────────────────  ───────
   Balloon activity                                     6
  ───────────────────────────────────────────────  ───────
   Pyrotechnics/fireworks                               6
  ───────────────────────────────────────────────  ───────
   Glider activity                                      5
  ───────────────────────────────────────────────  ───────
   Hazard/disaster/firefighting                         4
  ───────────────────────────────────────────────  ───────
   Military operations                                  1
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Determine access requirements for the FAA SUA gateway.
- [ ] #2 Preproc/core ingest activation schedules when credentials/data are available.
- [ ] #3 Airspace inspection/status can show active or scheduled status.
<!-- AC:END -->

