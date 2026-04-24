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

## How To Use

- Put the latest rendered overlays in `/tmp/procedure-plots-after/`.
- If the case is still confusing, also render step frames.
- Compare the decoded path against the actual plate, not just against prior output.
- If a case is known-bad before a refactor, keep it here until we have rechecked the
  post-refactor rendering by eye.
