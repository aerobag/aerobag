  Need a data-contract check for procedure-to-plate mapping.

  Current state:

  - flight-plan procedures come from CIFP-style data (cifp_sid_star_app)
  - plate folder/catalog entries already have human labels like ILS OR LOC RWY 34
  - UI currently only has exact procedure intent keys:
      - airport_id
      - procedure_id like I34
      - kind
  - there is no exact published join from that procedure intent to a specific
    plate/chart asset

  Why this matters:

  - procedure containers in PLN should show human plate-style names, not terse
    CIFP ids
  - procedure containers should have View Plate that jumps directly to the exact
    plate in PLT
  - I do not want to implement this with UI heuristics / string matching

  Please investigate:

  1. Does the raw FAA source material already contain an exact procedure-to-plate
     association?
  2. If yes, can preprocessor publish it into product data/resource index/chart
     catalog?
  3. If no, is there still a reliable preprocessing-time join we can compute once
     and publish centrally?

  Useful observations:

  - The nice labels are already in plate/chart catalog data.
  - The terse ids are in CIFP procedure data.
  - The missing piece is an exact machine-readable association between them.

  Requested output shape would be something like:

  - for a procedure intent key (airport_id, procedure_id, kind[, transition info
    if needed])
  - publish:
      - exact chart_id
      - exact human chart_label

  That would let UI:

  - display the friendly name on procedure containers
  - implement View Plate with no guessing

------------------------------------------------------------------------------
Done! Next:
------------------------------------------------------------------------------

  Use the new cifp_tpp_matches table in main.db as the authoritative CIFP↔TPP join. Query by (airport_id, cifp_id) to get one or
  more matching plates; if multiple rows come back, prefer is_primary=1, which currently selects the public plate over SA/CAT/copter
  variants. match_kind='unique' means a single confident join, match_kind='multiple' means multiple confident joins for the same
  CIFP, and absence of rows means the join is still unknown rather than “no plate exists.”

