# CIFP To TPP Matching

## Goal

We want a reliable relation between:

- CIFP approach identifiers, keyed by airport
- TPP plate identifiers, keyed by airport

The intended use is:

- given a selected CIFP procedure, find the corresponding TPP plate
- allow some CIFPs to map to multiple TPPs when the FAA really publishes multiple plates for the same procedure
- prefer the public plate later in product/UI code when both public and special-aircraft variants exist

The matching work in this document is exploratory and currently lives in:

- [`product/preprocessor/scripts/iap_match_audit.py`](/root/aerobag-preprocessor/aerobag/product/preprocessor/scripts/iap_match_audit.py)


## Scope

This work is intentionally about matching with data we already ship:

- published TPP `package-assets.json`
- published `main.db`
- airport aliases

We explicitly did **not** add a new upstream FAA source to solve this. We investigated that path, but it did not produce a clean direct join.


## What We Learned First

### 1. Airport identity was partly self-inflicted

The d-TPP XML carries:

- `apt_ident`
- `icao_ident`
- `procuid`

We were discarding `icao_ident` and `procuid`.

That was fixed in product code before this writeup:

- TPP package manifests now preserve `icao_airport_id`
- TPP package manifests now preserve `procedure_uid`
- resource-index/catalog plate records now preserve those fields too

This did **not** solve the procedure join, but it removed an avoidable airport-ID mismatch.


### 2. `procuid` is real, but it does not join to our current CIFP import

We verified:

- d-TPP `procuid` is a real FAA procedure identifier
- `(icao_ident, procuid)` is effectively unique on the TPP side

We also checked whether our imported CIFP side already had the same identifier hidden in some field.

Result:

- no obvious direct join field was found in our current CIFP import
- `file_record_number` is not the answer
- naive grep of raw CIFP text for `procuid` values only finds accidental numeric substring matches

Conclusion:

- preserving `procuid` was still the right move
- but the TPP-to-CIFP procedure join still has to be inferred from names and CIFP procedure tokens


### 3. Public FAA NDBR/IFP data is interesting but not yet worth pivoting to

We inspected the public FAA IFP/NDBR surface.

What we found:

- it is public
- it is procedure-centric
- some NDBR filenames appear to embed ARINC-ish tokens

Why we did **not** pivot:

- it did not yield a clear, universal, documented join to our CIFP identifiers
- using it would still require brittle heuristic parsing

So we stayed with the local-data-only matcher.


## Data Model We Settled On

This was the most important structural shift.

The problem is **not** a clean bijection.

We now model the relation this way:

- a TPP may claim multiple CIFPs
  - example: `ILS OR LOC ...`
- a CIFP may claim multiple TPPs
  - example: public plate plus `SA CAT` variant
- some CIFPs are legitimately only represented by copter plates
- some CIFPs remain unresolved

The audit classifies CIFPs into:

- uniquely bound
- multiply bound
- copter-only residual
- unresolved

This was the right model. Earlier attempts that forced a one-to-one relationship overstated the failure count.


## Current State

Latest audit result at the time of this document:

- airports considered: `3017`
- uniquely bound CIFPs: `10047`
- multiply bound CIFPs: `269`
- copter-only residual CIFPs: `16`
- unresolved CIFPs: `52`
- airports with unresolved CIFPs: `40`

That is after:

- filtering obvious non-approach CIFP noise
- separating copter-only residuals
- teaching the matcher most of the important family patterns


## Important Audit Policy Decisions

### Ignore airports with no CIFP approaches

Example pattern:

- military fields with TPP plates but no CIFP approach rows

We do not count those as matching failures.


### Ignore visual plates

Visual plates are not part of the first target problem:

- “map from a selected CIFP to a TPP plate”

So visual plates are ignored in relation classification.


### Treat public and SA/CAT variants as legitimate multi-bindings

Example:

- public `ILS OR LOC RWY 06`
- special `ILS RWY 06 (SA CAT I - II)`

Those are not duplicates in the harmful sense. They are distinct published plates for the same underlying procedure.

So the audit records both, and later product logic can prefer the public plate.


### Treat copter-only variants as a separate residual class

Examples:

- public `ILS Z OR LOC Z RWY 05`
- only copter `ILS Y OR LOC Y RWY 05`
- CIFP contains both `...-Y` and `...-Z`

Those unresolved `Y` or `Z` CIFPs are not “unknown.” They are represented only by a copter plate we intentionally do not use.

So these are tracked as:

- `copter_only_cids_total`

instead of inflating unresolved.


### Filter CIFP `route_type='T'`

This was a major cleanup.

Named procedures like:

- `JACKY1`
- `KEENE6`
- `NACHE4`
- `SFO4`

were showing up in the CIFP set and polluting the unresolved count.

Inspection showed these are `route_type='T'` records in `section_code='P' / subsection_code='D'`, i.e. not approach procedures we should match to TPP approach plates.

So the CIFP-side loader now excludes:

- `route_type IN ('1','2','3','4','5','6','T')`

That single filter dropped unresolved CIFPs from `162` to `99`.


## Heuristic Decisions We Added

This is the core history future sessions will need.

### Composite public plates should claim multiple CIFPs

Example:

- `ILS Z OR LOC Z RWY 13`

should claim:

- `I13-Z`
- `L13-Z`

Likewise:

- `ILS OR LOC ...`
- `ILS OR LOC AND DME ...`

This was the first big conceptual fix.


### Straight-in nonprecision runway procedures map to `Sxx`

Observations from real plates:

- `VOR OR TACAN RWY 12` has minima line `S-12`
- `VOR OR TACAN RWY 30` has minima line `S-30`

So runway-based nonprecision procedures like:

- `VOR RWY xx`
- `VOR OR TACAN RWY xx`
- `TACAN RWY xx`
- `NDB RWY xx`

should emit `Sxx` candidates.


### DME-defined straight-in nonprecision procedures map to `Dxx`

Observed at `KJAN`:

- `VOR AND DME OR TACAN RWY 16L`
- `VOR AND DME OR TACAN RWY 16R`
- etc.

corresponded to:

- `D16L`
- `D16R`
- ...

So runway-based `VOR AND DME ...` procedures emit `Dxx`.


### `VDM-*` means VOR/DME circling

Observed cleanly at `KHQU`:

- TPP: `VOR AND DME-A`
- CIFP: `VDM-A`

So circling `VOR AND DME-*` maps to `VDM-*`.

We later generalized this to:

- `VOR AND DME OR GPS-*` -> `VDM-*`


### Plain `GPS RWY xx` maps to `Pxx`

Observed at:

- `KPTV`
- `26N`
- `5J9`
- `KCJJ`

Examples:

- `GPS RWY 12` -> `P12`
- `GPS RWY 30` -> `P30`

This is distinct from:

- `RNAV (GPS) RWY xx` -> `Rxx`


### `RNAV (RNP)` maps to `H*`

Observed repeatedly:

- `RNAV (RNP) Z RWY 16C` -> `H16CZ`
- `RNAV (RNP) Y RWY 30` -> `H30-Y`
- plain `RNAV (RNP) RWY 26L` -> `H26L`

We also widened the recognized suffix set beyond `X/Y/Z` to include real FAA variants such as:

- `U`
- `W`


### Combined runway labels need splitting

Observed at `KLAS`:

- `VOR RWY 26L AND R`

must be treated as two runway claims:

- `26L`
- `26R`

Likewise for plain `RNAV (RNP) RWY 26L AND R`.


### `ILS PRM` is its own family shape

Observed at `KDTW`.

PRM labels were previously all no-heuristic. That was wrong.

We added conservative rules so:

- `ILS PRM RWY xx` can claim `Ixx` and `Lxx`
- `ILS PRM Y/Z/... RWY xx` can claim `Ixx<variant>` and a plain `Lxx`

This addressed the DTW PRM residue without pretending PRM is identical to ordinary `ILS OR LOC`.


### `VOR OR TACAN-A/B` and similar circling forms are not special

Observed at:

- `PHNL`
- `KMOB`
- `KAVX`

We added:

- `VOR OR TACAN-A` -> `VOR-A`
- `VOR OR GPS-A` -> `VOR-A`
- `VOR AND DME OR GPS-B` -> `VDM-B`

This removed a lot of late-stage circling residue.


## Things We Explicitly Did Not Force

These matter because future work should not “fix” them incorrectly.

### Copter plates are not used to satisfy public TPP matching

We only classify them as:

- copter-only residual

We do **not** silently promote copter plates into the normal match set.


### We did not force missing public Y/Z variants

Examples:

- `KOTH`
- `KMKT`

In these cases, CIFP may contain both `Y` and `Z`, but only one public plate exists and the other variant exists only as a copter plate.

That is not a matcher bug.


### We did not claim a direct `procuid` to CIFP join exists

It may exist somewhere upstream in FAA systems, but not in the data we currently ingest.


## What Still Looks Unresolved

At `52` unresolved CIFPs, the residue is now mostly one of these classes.

### 1. Weird suffix grammar

Examples:

- `I09RV`
- `I17-V`
- `L17`
- `R027`
- `R31-X`
- `R250`

These look like real parser gaps, not fundamental ambiguity.


### 2. Residual special procedure families

Examples:

- `Q24R`
- `Q22`
- `Q07-Z`
- `Q32`

These likely encode a coherent family we have not named yet.


### 3. Some remaining runway ILS/LOC mismatches

Examples:

- `I10`, `L10`
- `I01L`, `L01LZ`
- `I06`, `L06-Z`
- `I31`, `L31-Z`

These need direct inspection airport-by-airport.


### 4. A few named non-`T` leftovers

Examples:

- `CPTAL1`

This may be another filter issue or a genuine special-case approach family.


### 5. `GPS-A` style circling residue

Example:

- `S59`: `GPS-A`

This likely wants another small circling-family rule.


## High-Value Next Airports

If a future session wants to keep reducing the `52`, these are good places to start.

### `KPHL`

Current unresolved:

- `I09RV`
- `I17-V`
- `L17`

Why it matters:

- likely one coherent suffix-grammar problem
- not a big pile of unrelated leftovers


### `KJFK`

Current unresolved:

- `R027`
- `S13L`
- `S13R`

Why it matters:

- small set
- probably a real family rule hiding there


### `KEWR`

Current unresolved:

- `J22L`
- `J22R`

Why it matters:

- only two left after the copter-only separation
- likely a clean family-code rule


### `PGUM`, `PTKK`, `PGSN`, `PASD`

Current unresolved:

- `Q24R`
- `Q22`
- `Q07-Z`
- `Q32`

Why it matters:

- probably the same family
- likely the highest leverage remaining bucket after the big continental airports


## Recommended Next-Step Workflow

For a future session:

1. Run the audit script and capture the current top unresolved airports.
2. Pick one airport with a small coherent unresolved set.
3. Inspect:
   - CIFP identifiers
   - TPP labels
   - matcher candidate groups
4. Decide whether the residue is:
   - a real matcher gap
   - copter-only/public mismatch
   - CIFP-side noise/filtering issue
5. Add only the smallest justified rule.
6. Re-run the audit and confirm the rule did not cause broad regressions.

This work got better every time we followed that discipline and got worse every time we guessed too broadly.


## Current Script Behavior To Preserve

The current script should continue to preserve these distinctions:

- unique binding
- multi-binding
- copter-only residual
- unresolved

That framing is important. It is the reason the remaining `52` now mean something real.
