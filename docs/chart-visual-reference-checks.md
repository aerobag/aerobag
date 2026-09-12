# Chart Visual References

## Contract

An approved image records the chart at the last manual cutline/georeference
review. Comparing successive cycles is not sufficient: small displacements
must not become accepted by accumulation. Builds NEVER create or update approved
references. Approval is an explicit operator action against a rendered candidate,
and requires that the candidate's cutline/georeference still matches the repo.

References live with `product/chart-metadata`, not in the disposable source cache.
They include PNG views, sampling windows, source geometry/provenance, and a
versioned rendering recipe. Candidates and before/current/difference reports
live under artifact `state/chart-quality`. Reports are written before rendering,
and Pipeline Health reads them independently of successful products.

## Checks

- Fixed-window, mildly blurred RGB comparison; never align away displacement.
- Separate interior and boundary-ring change fractions, including outside context.
- Native-resolution neighborhoods around manually supplied inset controls.
- Explicit missing-reference, metadata-change, source-geometry, unreadable-image,
  and invalid-reference states. None count as a verified match.
- The required per-map-inset `projection_wkt` is part of its approved definition.
  Exclude-only regions need no geographic calibration, but their pixel boundaries
  are fingerprinted with the same critical source-sheet quarantine policy.
  Changing it invalidates approval and stale candidates even if every pixel is
  unchanged. A change in the FAA source's CRS also alarms; it never changes the
  pinned inset projection automatically. Migrating implicit projections to
  explicit metadata requires reapproval, not rewritten approval hashes.
- Main-map changes request review. Strong manual-inset drift or a failed manual
  calibration check quarantines the whole source sheet. Unreviewed initial references warn, so initial rollout
  does not silently approve anything or prevent generating review candidates.

The current recipe uses a 512-pixel longest-side overview, 4% context around
the cutline, and 128 x 128 native-pixel patches around every inset control.
After a 3 x 3 blur, a pixel changes if any RGB channel differs by more than
20/255. Changed-area warning/critical thresholds are 4%/18% for the interior,
10%/30% for the boundary ring, and 4%/15% for a control patch. Main-map pixel
changes are warning-only because their map placement still comes from the FAA;
inset changes can be critical because placement is manually calibrated.
Both reported scores and thresholds are available in the review report.

TAC/Flyway builds check their Sectional-sourced insets too, using the exact same
SEC reference as the parent's exclusion. Checks use the builder's source-tree
overlay order, including duplicate filenames shipped in multiple FAA archives.

## Publication Quarantine

The report protocol is schema 2; approved image references remain schema 1.
Every consuming family checks **all enabled manual insets on each source it
uses**, including siblings targeting other layers. A critical manual-inset result
quarantines that source TIFF: the parent cutline, all sibling overlays, and its
reference/legend extracts are omitted. This is deliberately not inset relocation:
an old polygon cannot prove where newly moved pixels are. Independently
georeferenced FAA detail TIFFs are separate sources, not manual siblings.

The checker returns a typed publication decision with the exact excluded metadata
and source filenames. The build applies it only to its fresh disposable inputs;
authored definitions, enabled flags, approval images and source-cache bytes are
never edited. The decision participates in the render-cache fingerprint. The
packager consumes that completed render directly, including the TAC/Flyway
bundle, rather than rediscovering a cache entry. Empty quarantined layers produce
empty tile sets, never old tiles. Review/approval changes are checked even on cache
hits. A checked-in corrected and approved definition restores the source on the
next build; the publisher never approves or guesses on the operator's behalf.

Other sheets may publish while Pipeline Health remains **CRITICAL** and explicitly
names the omitted source sheets, with before/current/difference reports and the
complete metadata exclusion list. This reports the latest build decision, not a
claim that it has already been deployed. Checker-wide failures (such as missing
GDAL or malformed layout inventory), or critical errors outside any identified
manual-inset source, still block publication: there is no safe quarantine scope.

Pipeline Health reports unresolved warnings and criticals, not increases over the
last build. Alerts remain until a new check passes with an explicitly approved
reference. The latest attempt for each family is shown, including its cycle and
build identity; this is a build-quality signal, not a claim about deployed charts.

## Calibration And Limits

Only one SEC source cycle remained in the cache at implementation time. Synthetic
fixtures must distinguish label edits from translation, scale changes, and margin
intrusion, and exercise control-point changes. Real adjacent-cycle false-positive
rates cannot be claimed until we collect those pairs. Initial thresholds are
versioned, conservative, and recorded in every report for review.

This is a change detector, not an independent georeference verification. Blank
or repetitive scenery and changes below sampling resolution can evade detection.
Control patches reduce, but do not eliminate, that limitation. Approve references
only after checking placement, not merely because a change looks plausible.

## Operator Flow

### Sequential Review

Run the interactive reviewer on a trusted operator machine, separately from the
read-only Pipeline Health service:

```sh
/usr/bin/python3 tools/chart_cutline_audit.py --review-server \
  --report-root /path/to/artifact-root/state/chart-quality \
  --state-dir /path/to/persistent/chart-review \
  --metadata-root product/chart-metadata --reviewed-by 'operator name' \
  --editor-url http://localhost:8093 --port 8094 \
  --family-work SEC=/path/to/charts-sec-fetch/fingerprint/source
```

Supply additional `--family-work FAMILY=SOURCE_DIR` arguments for TAC, FLY,
ENR_L and ENR_H to enable their editor links and rerender buttons. FLY uses the
TAC source tree. TAC/FLY also require the SEC source tree. Run the existing
cutline editor with those same family mappings. The review server defaults to
loopback; `--host 0.0.0.0` is appropriate only on a trusted LAN. It writes repo
references and is not intended for public deployment. Mutations require its
per-process request token and reject cross-origin requests.

The reviewer presents one scrollable list **per input sheet**, not per output
region. A whole-source overview includes the margins and every defined main
cutline, enabled/draft navigable inset, reference-image extraction and legend.
Numbered colored outlines use the editor's shared CRS projector. Region buttons
name destinations separately from whether the region is excluded from the parent:
ordinary reference extraction does not itself mask the parent. These are current
metadata/build rules, not a claim about an already deployed product.
Both the left index and card heading show region approval progress separately
from inset-inventory completeness. The approval denominator counts reference
candidates, not inventory-only extracts or legends; approving a region updates
that progress immediately without changing the completeness decision.

- **Next sheet / N** and **Previous / P** move without recording a decision.
- **Approve region & Next** approves only the selected pinned region candidate.
- **Disapprove region & Next / D** rejects that candidate with a correction note.
- Clicking a numbered outline or region button selects its candidate, existing
  decision, comparison link and editor. Reference images, legends and drafts
  currently have inventory entries, not visual-reference checker candidates;
  their controls say so rather than inventing an approval.
- **All inset regions accounted for** records a separate source-sheet decision.
  It never approves cutlines. An approved main cutline never implies completeness.
- **Flag missing / unresolved regions** saves a sheet-level note, including
  unoutlined regions that cannot yet be selected. `/api/sheet-rejections` exports
  these notes. No automatic detection of unknown insets is claimed.
- Sheet decisions pin the full inventory definition and current immutable-cache
  source file identity (path/inode/size/mtime/ctime). Replacements or edits make
  completeness stale; metadata changes include legends and disabled drafts.
  They persist alongside, but do not alter, the older per-region decisions.
- **Flagged sheets** includes region rejections and sheet completeness issues;
  **All source sheets** includes already approved charts.
- **Edit this region** opens the existing editor at the right family and chart.
  After saving there, **Refresh after edit** renders a new candidate from the
  configured FAA source. It must be inspected and approved again.
- **Import latest reports** picks up newly completed checker reports without
  erasing rejection notes. Identical report pointers do not overwrite a
  locally rerendered correction on server restart.

SEC insets also checked by TAC/Flyway appear once, on their SEC source sheet.
Whole-source thumbnails are at most 2400 pixels on the long side. **Preload sheets**
defaults to the current sheet plus the next ten in the review queue; **All sheets**
warms the entire filtered queue, starting at the cursor, and **Visible only** stops
adding background work. The buffer counter includes current/visible sheets and
reports cached, loading and failed counts. Failed loads require **Retry failed
loads**, rather than looping on a broken network. Prefetching records no approvals.

At most two sheet loads run concurrently, with only one background load so a
newly selected sheet has capacity. Pending work is reprioritized as you navigate.
Images are drained into the browser's ordinary HTTP byte cache, not retained as
decoded image objects or blobs. Offscreen whole-sheet SVG/image objects are
released; only small geometry metadata is retained. The browser can evict cached
bytes, so this accelerates review but is not an offline installation guarantee.
Revision-addressed PNGs have private immutable caching for one day. Mutable
state and outlines remain uncached; JSON/SVG responses use gzip when accepted.
Native renders are serialized outside the review-model lock. The server's
in-memory geometry cache is bounded. Whole-sheet inventory images remain
separate from the immutable, pinned per-region approval evidence described below.

`review.json` in the review state directory records decisions, notes, candidate
digests and cursor position. Candidate images are pinned beneath that directory
so report GC cannot remove the evidence being reviewed. Keep this directory
outside `/tmp` and outside the checker's pruned `reports/` directories.
The scroll-through review draws its cyan outline as a vector overlay using the
editor's shared CRS projector and the pinned candidate's source georeference.
It does not reuse the low-resolution sampling-mask edge as display geometry.
This also corrects previews pinned before projection fixes without rewriting
their pixels, comparison masks, approvals or decisions. If the metadata no
longer matches the candidate, the old preview is explicitly labelled as previous
and must be refreshed before approval; it is never overlaid with a newer cutline.
`/api/rejections` exports the unresolved rejection list with editor links.
Previously rejected but newly rendered candidates stay on that list until
approved. Rejecting an accidentally approved candidate removes only that exact
reference; rejecting a new cycle preserves its older approved reference.

Approvals use the same function and repo representation as the CLI below.
They are not browser-local state. An old tab or an edited cutline cannot approve
an obsolete candidate. An unloaded/broken image cannot enable the approval
button. A per-region sample cache shares image decoding with comparison work;
refreshing one rejected region does not rebuild the entire chart family.

### Command Line

Use `tools/chart_cutline_audit.py --quality-help` for check/approve arguments.
Generate a report, inspect before/current/difference views and control patches,
and adjust cutline/georeference in the existing editor if needed. Regenerate the
candidate after editing, explicitly approve it, and commit the reference with the
metadata. Rerun the check/build to clear the alarm. Missing historical sources are
not reconstructed or silently substituted.

For example, from the repository root (substitute the retained source path):

```sh
/usr/bin/python3 tools/chart_cutline_audit.py --quality \
  --source-root /path/to/charts-sec-fetch/fingerprint/source \
  --family SEC --chart 'Chicago SEC' --cycle 2610 \
  --source-id fingerprint --output /tmp/chart-review/SEC
```

Open `reports/<report-id>/index.html` below that output directory. Each region
page includes the exact approval command for its candidate:

```sh
/usr/bin/python3 tools/chart_cutline_audit.py \
  --approve-reference /tmp/chart-review/SEC/reports/REPORT/REGION/candidate.json \
  --reviewed-by 'operator name'
```

That writes `product/chart-metadata/SEC/visual-references/<region-id>.json`.
Commit that file alongside any metadata edits. Changing the cutline after
generating the candidate invalidates the candidate; rerender before approving.
Approval records reviewer, timestamp, source identity, cycle, metadata hash,
and a hash of the full reference. Rechecking the same pixels in a new cycle
does not change it. This bootstrap requires actual review of the retained
source; it does not claim to recover a GC'd historical reference.

The official product build invokes this same checker before each chart process,
even on a tile-cache hit. Reports go to
`<artifact-root>/state/chart-quality/<family>/`. Pipeline Health provides a
review link via `/pipeline-health/chart-quality/`. The check needs the existing
GDAL Python bindings and NumPy, not any additional image-comparison package.

Reports retain the latest and previous two completed attempts per family. The
approved references are not subject to artifact GC. A checker crash leaves a
visible incomplete attempt rather than the previous successful status.

Regression tests:

```sh
/usr/bin/python3 -m pytest tools/test_chart_visual_references.py \
  tools/test_chart_visual_review.py \
  tools/test_chart_cutline_editor.py \
  product/preprocessor/scripts/test_pipeline_health.py
```

The `preprocessor-cli` Rust test
`chart_quality_failure_is_reported_before_tiling_or_publication` drives the real
build entrypoint and proves failed checks leave a readable report without
preparing a render cache or publishing packages.

A disposable browser journey also blocks subsequent image downloads after
prefetch and proves the next sheet still paints from cache without approving it.
It clicks real visible controls, checks approval and rejection persistence across
reload, and tests the narrow layout. It only writes
synthetic fixture metadata, never the operator's references:

```sh
AEROBAG_WEB_WORKSPACE_DIR=/path/to/web-target/workspace \
  node tools/test-chart-visual-review-browser.mjs
```

The target workspace must have the existing web test dependencies installed.
Screenshots go to the printed temporary directory. Python persistence and HTTP
tests are included in ordinary CI; this headless-Chrome journey is an explicit
additional UI check.
