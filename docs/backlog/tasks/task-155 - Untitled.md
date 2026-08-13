---
id: TASK-155
title: active/standby flight plan
assignee: []
created_date: '2026-08-05 00:01'
labels:
  - cat:ui-affordances
dependencies: []
state: someday
ordinal: 155000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
One downside of these invariants is they get in the way of the pilot playing around with 
  alternate routes while leaving a live FP in place. Hmm. Not sure how much I care. Is there   
  an established UI pattern for this problem? Or do you have a clever suggestion? Or maybe the
  pilot can just pull out another tablet (and turn off crossfill :grimacing:)        



  The established pattern is active versus secondary/stored flight plans. Garmin provides a
  flight-plan catalog where a stored plan can be edited independently and later activated,
  replacing the active plan. Honeywell systems explicitly support primary and secondary plans.
  Garmin G1000 NXi guide (https://static.garmin.com/pumac/190-02178-00_B.pdf), Honeywell Primus
  Apex
  (https://prod-edam.honeywell.com/content/dam/honeywell-edam/aero/en-us/products/cabin-and-cockpit/avionics/integrated-flight-decks/primus-apex-integrated-flight-deck/documents/hon-aero-a60-1155-000-001-primus-apex-viking-twin-otter-brochure-en.pdf?download=false)

  I’d implement one SECONDARY slot:

  - COPY ACTIVE or NEW SECONDARY
  - Active plan continues driving CDI, guidance, and ETAs.
  - Secondary can be edited and displayed for comparison.
  - ACTIVATE atomically swaps it into the active slot, retaining the previous active plan as
    secondary for reversal.

  - Crossfill both named slots independently; receiving a secondary plan must never disturb
    active guidance.

  - Keep the attachment invariants within each plan. Dependency-changing commands can later
    become explicit atomic operations, such as “make this the origin and remove the current
    SID.”

  That gets most of the benefit without building a whole flight-plan catalog or making the
  pilot carry another tablet.
<!-- SECTION:DESCRIPTION:END -->
