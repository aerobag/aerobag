# NMS collector recovery

The collector retains rejected raw updates in `state.sqlite`. Parser fixes must
not just append those old updates to the current set: a later cancellation or
replacement may already have withdrawn the rejected notice.

On startup, with its normal exclusive collector lock held, the collector checks
for newly parseable rejected updates. Recovery reconstructs a scratch state from
the retained Initial Load baseline and all subsequent accepted and newly
parseable updates, using the original poll order, within-poll ordering, and
expiry boundaries. Only the final difference is applied to current state, in one
transaction with its canonical change journal and accepted-payload markers.
Neither the poll cursor nor the source epoch is reset. Rejected payloads remain
as evidence; an accepted copy of the same hash makes repair idempotent.

The repair checks the baseline's source epoch and every replayed payload's hash.
It also verifies that replaying accepted history alone reproduces today's state
(apart from subsequent expiry), before trusting the reconstructed difference.
A missing baseline, damaged history, ambiguous cancellation, or failed database
transaction aborts rather than publishing a partially reconstructed state.
Updates from before the installed baseline are superseded by that baseline.
Unparseable updates remain quarantined.

For an explicitly coordinated offline repair, using a binary with the parser
fix and an existing collector state:

```sh
nms-notams-fetch repair-rejected --state-root /path/to/nms-notams
```

Stop the owning collector first, preserve a consistent SQLite backup, and retain
the capture directory referenced by its `baseline_capture_path` metadata. The
command refuses to run while the collector holds its lock. It makes no FAA
requests. Restart with the fixed producer afterward so future updates also use
the corrected parser. A repair alone does not upgrade an old producer binary.

## Missing-year upstream variant (October 17, 2026)

NMS DOMESTIC and FDC updates began omitting `event:year` while retaining
`event:issued`. For this variant only, the normalizer derives the four-digit
identity year from the original issuance instant in UTC, never from receipt
time, `lastUpdated`, or effective dates. An explicit year stays authoritative;
an invalid explicit year still fails. Missing/invalid issuance or a conflicting
FDC year digit remains an error. This changes XML input support, not the
canonical record contract.

Two exact rejected FAA payloads are captured in the sibling
`preprocessor-live-feeds/tests/fixtures/` directory (a trailing newline was added):

| Fixture | NMS ID | First rejected (UTC) | Original payload SHA256 |
| --- | --- | --- | --- |
| `nms_missing_year_domestic.xml` | 7292862494853778 | 2026-10-17 04:58:03 | `5e8ec9acd6b8906ec1f4da4f4d42bbf4205359f5a348d50161a15a101539ff15` |
| `nms_missing_year_fdc.xml` | 6046133424189660 | 2026-10-17 10:10:03 | `8625b8facc835cc443a66e8d64e0831ea7eb4a0caee2780a76a650d3b4690682` |

These are public aeronautical messages, captured from production's retained
`rejected_updates`, not synthetic schema examples. Tests also cover year rollover,
UTC offsets, missing/invalid issuance, explicit-year validation, and FDC identity
conflicts. Recovery tests exercise cancellations, replacements, expiry, corrupt
history, rollback, locking, publication continuity, and repeat execution.
