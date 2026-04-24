# Plates To Check

These are real procedure/plate cases that are known-fragile or recently fixed.
They should be visually inspected against the published plate after any
intercept/capture refactor, even if the materialized-path tests still pass.

## Current Cases

- `KCOE I06 / GEG`
  - reason: missed-approach `FA` overshoot and same-fix continuation on `COE R-350` outbound then `R-170` inbound
  - current artifact stem: `KCOE_I06_GEG`

- `KCOE L06 / GEG`
  - reason: sibling of the same `FA` + course-capture pattern as `I06`
  - current artifact stem: `KCOE_L06_GEG`

- `KMSO I12-Y / EMIBE`
  - reason: visually sensitive missed-approach / intercept sequencing; previously suspected of hairpin behavior
  - current artifact stem: `KMSO_I12-Y_EMIBE`

- `KMSO I12-Y / JIROS`
  - reason: sibling of the same `KMSO I12-Y` missed-approach / intercept pattern
  - current artifact stem: `KMSO_I12-Y_JIROS`

## Nationwide Audit Exemplars

- `03D R12 / IDIXE`
  - bucket: runway RNP heading continuity
  - reason: representative of the dominant nationwide failure family; continuity break at the runway transition

- `05U R18 / JEBEG`
  - bucket: runway RNP heading continuity
  - reason: same family, but with a much more dramatic near-hairpin runway handoff

- `02G R25 / EWC`
  - bucket: zero-length arc endpoints
  - reason: representative zero-length arc construction failure in runway RNP geometry

- `KACK I06 / MVY`
  - bucket: non-RNP heading continuity
  - reason: sharp continuity failure away from the runway RNP cluster; useful to keep us honest on non-`Rxx` procedures

- `KBED I11 / BRONC`
  - bucket: non-RNP heading continuity
  - reason: especially suspicious because the validator is using the tighter `10 deg` threshold at `LOBBY`

- `KATL I27R / YOUYU`
  - bucket: small path continuity gaps
  - reason: representative tiny `0.04 nm` stitch gap at `MMCAP`

- `KBJC I30R / ROKXX`
  - bucket: zero-length display path
  - reason: representative degenerate `LAWNG -> LAWNG` leg with no display path

- `KDFW I17R`
  - bucket: zero-length segment
  - reason: representative explicit zero-length rendered segment

- `KMSO VOR-A / ALTON`
  - bucket: zero-length display path
  - reason: non-runway-procedure example of a degenerate same-fix leg

## How To Use

- Put the latest rendered overlays in `/tmp/procedure-plots-after/`.
- If the case is still confusing, also render step frames.
- Compare the decoded path against the actual plate, not just against prior output.
- If a case is known-bad before a refactor, keep it here until we have rechecked the
  post-refactor rendering by eye.
