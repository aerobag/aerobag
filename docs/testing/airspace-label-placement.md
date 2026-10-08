# Airspace label placement and candidate budget

Measured 2026-10-08 against cycle 2610 B/C/D airspace, using NAV28 publication
`main-d6176c606ef0/20260917T172525Z`. No NAV publication was rebuilt for this audit.

## Failure and placement contract

Seattle's southern 70/60 shelf is
`airspace:data_2610:b:sea:class_b:4005`. At an 800×1100 logical-pixel viewport,
center 47°/-122.3°, zoom 11, it spans about 591 pixels horizontally. Its preferred
altitude label overlaps WAALP's label. The producer had published other usable
anchors in neighboring tiles, but core discarded them before collision checking.

Core now retains all visible ranked candidates. It first places preferred labels
using the existing priorities, preserving point labels and shelf labels that fit.
Only then does it try alternatives for suppressed airspace labels. It still draws
at most one label per feature and leaves it suppressed if every candidate collides.
Fallback bounds use the same rotation, display density and padding as preferred
labels. Do not fix this by hiding nearby fixes or relaxing collision bounds.
The two passes are intentional: trying a fallback immediately after its preferred
anchor collides lets an earlier shelf steal a later shelf's usable preferred
position. Candidate preference follows published rank, independently of tile or
record arrival order.

The producer keeps the existing 10×10 candidate grid and preference order, then
appends nonduplicate candidates from a 20×20 grid. Polygon-containment sampling
stays unchanged. Candidates still must pass the existing interior/nested-shelf
checks, 50-pixel bounding-box gate and 25-pixel clearance gate.

Each feature publishes at most **four candidates per tile per zoom**: the preferred
anchor, then points chosen greedily to maximize distance to the closest selected
point in projected map coordinates. Rank breaks distance ties and remains the
client's placement preference. Simply taking the first four ranks often gives
several nearby points that collide with the same label.

## Coverage and cost

The audit exercises 1,287 B/C/D features, each centered in an 800×1100 north-up
viewport. Its zoom fits the bounding box within 640×880 pixels, clamped to 8–14.
It runs the production core query with actual published point features, boundaries
and labels. A comparison without point features identified 156 labels suppressed
by point-label collisions. The earlier 137-recovery estimate considered each
feature separately; the actual simultaneous fallback pass also recovers 137.

| Samples / candidates per feature-tile | Recovered out of 156 | Visible out of 1,287 | Published label records | Warm query p95 |
| --- | ---: | ---: | ---: | ---: |
| Existing 10×10 / 1, with core fallback | 137 | 1,240 | 19,047 | 0.454 ms |
| Existing 10×10 / 4 spread | 150 | 1,253 | 57,705 | 0.476 ms |
| **10×10 + 20×20 / 4 spread** | **153** | **1,258** | **80,794** | **0.484 ms** |
| 10×10 + 20×20 / 8 spread | 155 | 1,260 | 145,905 | 0.509 ms |

Publishing every old-grid candidate recovers only 151; even 16 per tile offers
no improvement over 8. Publishing all 695,824 candidates from the combined grids
recovers 155, the same as the bounded eight-candidate experiment. Four candidates
is the chosen compromise: eight publishes 81% more records to recover two more
labels in this sample.

The final Rust producer independently confirms the chosen four-candidate result.
It loses none of the 1,091 labels visible before the fix, and point-label counts
are unchanged in every scenario. The three unrecovered cases from the original
156 are Atlanta 125/30 (`...:atl:class_b:971`), Minneapolis 100/40
(`...:msp:class_b:4131`), and San Francisco 100/40 (`...:sfo:class_b:3808`).
These are outcomes at the specified viewports, not claims that those shelves can
never be labeled. Other initially absent labels have causes outside the original
156-case point-collision subset.

The chosen label arrays total 13,552,093 compact JSON bytes, versus 3,182,484
originally. Compressing only those arrays in independent 64 KiB XZ-6 blocks gives
1,567,072 versus 388,232 bytes: an estimated **1.12 MiB** increase. This is a
label-only storage proxy; actual NAV tree layout, neighboring values and package
size were not measured. The eight-candidate proxy is about 2.55 MiB in total.

Timing is optimized native Rust on this host, with inputs already loaded, 20
queries per viewport; the table reports the 95th percentile of per-viewport
medians. It includes full overlay projection and collision work, excludes I/O and
JSON decoding, and is not an Android-device or browser-frame benchmark. The chosen
policy increases p95 by about 30 microseconds (7%) over core fallback using the
existing publication. Candidate generation and selection run in the preprocessor.

## Reproduction and regression protection

Local evidence is in `/root/aerobag-six/airspace-label-investigation/`, including
`national-audit.jsonl`, `final-comparison.json`, `final-*.jsonl`, producer candidate
and tile exports, and the standalone Rust/Python experiment sources. The core
probe reads the actual NAV directory through `NavKvDirectoryReader`/`NavKvRoot`,
loads all requested inputs, then substitutes only B/C/D label arrays. A reconstructed
one-candidate control produced identical labels and counts to published data in
all 1,287 cases. Experimental variants use the production source parser, candidate
ranking and gates; the final selected policy was checked with Rust's actual
selection helper before accepting its measurements.

The source inputs were `Additional_Data/Shape_Files/Class_Airspace.shp` and airport
points from cycle 2610's `intermediate-sqlite.db`. To repeat against a later cycle,
generate the same feature-centered scenarios and compare complete simultaneous
placement, not independent counterfactual recoveries. Compare caps 1/2/4/8/all and
both rank-only and spatially spread sets. Preserve existing preferences when
testing denser grids so the experiment measures fallbacks rather than an unrelated
change of preferred anchors.

Regression tests cover fallback rank order, preserving already-visible labels,
exhausted alternatives, rotations 0°/40°/90°, densities 1/2.625, bounded spatial
selection, input-order independence, retaining coarse preferences, and excluding
nested shelves. The ordering regression puts the shelf needing a fallback before
the shelves whose preferred positions it must preserve, and reverses the input
records to verify stable placement. The core suite passed 1,048 tests (28 ignored);
the vector producer passed 23. Physical Android, browser rendering and release
journeys were not run.

The core improvement works with current publications. Denser alternatives arrive
with the next ordinary NAV build. The vector source fingerprint invalidates the
producer cache; the existing label-array contract supports these extra records,
so no NAV contract/version bump is required.
