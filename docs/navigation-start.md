# Starting Navigation From the Current Position

START replaces STOP while guidance is inactive. Core chooses an existing
guidance detail and uses the same activation operation as the flight-plan row.
This is neither Direct-To nor simulated sequencing through earlier legs.

## Selection Policy

- Use current ownship position and cached guidance polylines only; selection
  performs no NAVDB reads or network requests.
- Find the closest point on each finite path, including curved procedure
  geometry. Reject completed details and candidates farther than 5 nm.
- Use true track only with finite speed of at least 5 kt. Reject candidates
  more than 100 degrees opposed to that track. Rank distance plus a heading
  penalty of 2 nm per 90 degrees, using the local path tangent.
- Candidates within 0.5 nm-equivalent score of the best are ambiguous. An exact
  stopped-detail hint may break that close tie; it cannot override a clearly
  better current-position fit. Route edits, replacement, and reload discard
  the hint. Stopping Direct-To does not create one.
- Procedure discontinuities and manual-heading legs require exact remembered
  resumption. Restricted candidates participate in ranking so the chooser
  cannot silently jump over them to a worse ordinary leg.
- The published model does not distinguish approach from missed-approach
  phase. Until that metadata exists, **all approach details require exact
  remembered resumption or explicit row activation**. Do not infer phase from
  geometry. A new phase field would need the normal NAVDB contract migration.
- Exact resumption preserves suspension and its reason. Choosing a different
  detail uses normal Activate Leg semantics. START/STOP do not enter edit history.

These are conservative selection heuristics, not procedure guidance rules.
Uncertain choices remain the pilot's explicit decision through Activate Leg.

## Feedback and Tests

There is no additional picker. An unavailable START retains a core-provided
reason and tapping it shows the existing toast, e.g. "Active leg ambiguous;
activate a leg via its flight-plan row." The command revalidates against the
latest position; rejection at dispatch also shows a toast.

Core tests cover mid-plan entry, stale stop hints, crossings, reciprocal and
overlapping routes, low-speed track, finite and curved geometry, antimeridian
crossing, missing geometry, procedure subdetails, holds, vectors, approach
restriction, suspension preservation, edit history, and mutation-free failure.
The shared flight-plan release journey exercises STOP -> START -> STOP on the
same core-driven controls used by web and Android.
