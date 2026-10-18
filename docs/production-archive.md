# Rolling production archive

The deployment owns a continuous archive on `/mnt/aerobag-archive`. It does not
increase live-feed retention or use the production cache as its historical store.
This is a rolling investigation archive on the production host, not an off-site
disaster-recovery backup.

## Configuration and installation

The authoritative host-specific configuration is `deploy/aerobag-prod.json` in
the main repository. `aerobag-jon` is another checkout, not a separate deployment
repository. Credentials remain outside the source tree in `aerobag-credentials`.

The checked-in allocation is:

- `archive_enabled`: true.
- `archive_root`: `/mnt/aerobag-archive`, which must be a mounted filesystem.
- `archive_total_gib`: 1700, combined repository budget.
- `archive_cycles_gib`: 1400; the remaining 300 GiB belongs to live feeds.
- `archive_buffer_gib`: 16, maximum committed live-feed buffer payload/index size.
- `archive_reserve_gib`: 64, free-space reserve for writes and compaction.

Repository quotas include allocated Borg files, not logical uncompressed sizes.
The buffer and free-space reserve are additional to the repository budgets.
Borg cache files, directory/control-file overhead, and transient writes also need
headroom. The collector refuses to write when the reserve is reached; it never
deletes unarchived capture data to make space. An unmounted archive disk is an
error, not permission to fill the system disk underneath its mountpoint.

After this code is committed and integrated, the **operator** installs it using
the existing deployment command:

```sh
tools/prod_manage.py --reconcile
```

Reconciliation installs Borg 1.x, builds the small native NOTAM checkpoint helper,
installs `/etc/aerobag/archive.json`, and starts these independent units:

- `aerobag-archive-collect.service`: checks publication history every 15 seconds.
- `aerobag-archive-store.timer`: runs the Borg worker at boot and five minutes
  after each completed invocation. It archives sealed days and changed cycles.

Both workers run with low CPU/IO priority. Deployment/runtime fingerprints include
their sources and configuration. Changing `archive_enabled` to false disables
collection and the store timer. No production deployment was performed as part
of implementing this feature.

Both units retain `ProtectSystem=strict` and use `PrivateTmp=true`, so Borg's
startup self-tests have writable temporary space. The collector can write only
the archive and health trees (plus its private temporary directories); production
artifacts remain read-only. The store worker additionally has a single writable
mount of the artifact tree to make hardlinks from `published/` into
`state/archive-cycle-pending/`. Separate writable mounts of `state/` or `locks/`
would break those hardlinks with `EXDEV`, even on the same underlying filesystem.
The store code still treats published bytes as immutable; there is no copy fallback
or change to publication GC/retention.

## Live-feed capture

The collector discovers the providers serving production and sunset releases
from the controller's current generation and live-feed bindings. A shared
provider is captured once; staging-only providers are excluded. Before retiring
a provider, it attempts a final catch-up and seals its partial day.

Capture reads the same immutable compressed publications as clients, locally.
It deliberately does not rely on SSE: SSE can coalesce successive publications.
Delta products are followed through their version-manifest chains. Full-frame
products such as NEXRAD are scanned for all retained intervening versions.
Publication-file timestamps represent when data became available here, rather
than the upstream observation time. The archive preserves original manifests
and payload bytes; it does not regenerate METAR/TAF/TFR/obstacle deltas.

Each provider has an independent daily batch under `live-buffer/batches/`:

- `batch.json`: source identity, UTC day, beginning time, previous batch name.
- `index.sqlite`: ordered events, committed product heads, payload byte accounting.
- `events/<id>/`: an event's baseline, delta, or full replacement payloads.
- `gaps/`: recorded continuity problems, when any were detected.
- `sealed.json`: the batch is immutable and eligible for Borg.

Event `payloads` describes the dependencies actually retained. The accompanying
original version manifest can mention other producer resources that were not
needed, and were not captured. Directory baselines include all subordinate
compressed NavKv pages or NEXRAD tiles, not just a viewport's subset.
Their files are checked against authenticated producer ZIP inventories, so a
concurrent producer GC cannot quietly turn a partial directory walk into an
apparently complete archived baseline. NEXRAD's manifest also enumerates the
coarse levels absent from its offline ZIP profiles. This reads ZIP directories,
not inflated NavKv contents.

Daily NOTAM baselines are special: the producer can reference an older checkpoint
plus a suffix of deltas. `aerobag-archive-notam` uses the existing Rust
`NotamState` implementation to apply and validate that suffix, then emits a new
self-contained checkpoint. Only that daily baseline is recompressed. Normal
NOTAM updates are copied unchanged, just like the other products' deltas.

### Midnight and crash ordering

1. Under the collector lock, drain retained pre-midnight publications into the
   old batch. An update observed after midnight still belongs to yesterday if
   its publication preceded midnight.
2. Copy independent baselines for the new day from the old day's committed heads.
   Reconstruct the NOTAM checkpoint when necessary.
3. Flush payload files, commit the SQLite event/head transaction, and flush the
   batch directory before atomically replacing the active-batch pointer.
4. Seal yesterday. If the process dies between the pointer swap and sealing,
   restart finishes sealing from the new batch's previous-batch reference.
5. Re-read the live heads and catch up. Updates published during baseline copying
   therefore become new-day deltas instead of falling between the buffers.

Failed baseline creation leaves yesterday active. Uncommitted payloads and
unpublished baseline directories are cleaned up on restart. Event and head
updates share a transaction, so an interrupted event is either replayed or
already committed, never skipped by an independently advanced cursor.

Borg reads only sealed batches. After creation, `borg extract --dry-run` verifies
the entire saved unit's data and authentication without writing a second copy.
Only then is the buffer atomically moved to a committed-GC directory and removed.
Interrupted GC is resumed on the next store invocation. A failed create or
verification leaves the batch available for retry under the same archive name.
Retries rebuild a pre-existing named archive from the retained buffer: a failed
Borg create can leave a readable but incomplete archive after skipping a file.
Readability alone is not proof that every source file was captured.

Capture is not a substitute for unlimited producer history: sufficiently long
outages can lose intermediate states before capture resumes. Expired chains
resume from a full state and record a gap, rather than silently claiming
continuity. Absence longer than the shortest retained full-frame history also
latches a potential-gap alarm. The first day begins at collector startup, not
retroactively at midnight. A gap means historical completeness cannot be claimed
for that interval, even if current-state collection is healthy again.

## Cycle capture and retention

The Borg worker snapshots the production channel's `current_artifacts.json` and
the complete publication directories it names. It takes the shared release-state
lock (`locks/release-reconciler.lock`) while making hardlinks into
`artifacts/state/archive-cycle-pending` on the production data filesystem. This
pins the immutable bytes against publication GC without holding the state
lock during filesystem flushes or Borg's read. The ordering is:

1. Hold the state lock while selecting the generation, capturing metadata,
   and hardlinking every publication file. GC cannot remove sources mid-pin.
2. Release the state lock. The hardlinks now own independent references to
   immutable bytes, even if GC deletes the original publication names.
3. Flush the pending snapshot, then atomically write its `ready.json` marker.
   An interrupted flush never marks an incomplete snapshot ready.
4. Archive and verify it with Borg before recording completion and removing it.

The independent `store.lock` remains held throughout the pass, including health
updates. Only one pending snapshot is retained; a failed Borg operation leaves
it available for retry. A ready snapshot can be archived even while the
state lock is busy. Incomplete preparations are rebuilt on retry.

If the state lock is busy before a new pin, cycle capture is **deferred**
until the next timer invocation. This is not an archive failure and does not
advance the last-success timestamp. Live-feed archival and retention still run.
An overlapping store invocation exits without overwriting the active worker's
status. Other IO failures, including `EAGAIN` outside lock acquisition, remain
real failures.

In the opposite direction, reconciliation waits for an existing pin rather than
failing staging. It reports `Waiting for release state lock` in both its journal
and progress file, then reads desired/observed state only after acquiring the
lock. The separate `locks/release-controller.lock` rejects duplicate reconcilers
even while the first is waiting. This is a kernel lock wait, not a retry of any
build or deployment action; flush and Borg work remain outside the state lock.

Borg discovers content equality independently of cache identity. It preserves
shared chunks when an older archive is removed. Unchanged current manifests are
not archived again. Cycle polling captures publications observed by the worker;
it cannot promise to retain a publication replaced and GC'd entirely between
polls.

Each repository drops its oldest complete units when it exceeds its byte budget,
compacting after deletion until it is below a 90% low-water mark. The newest unit
is retained. If that unit alone exceeds the budget, the worker reports an error
rather than deleting the only retained state. Live days need no older day to
reconstruct their contents, so pruning has no delta-chain dependency across days.

Borg uses `--compression=none`: existing XZ/PNG/ZIP bytes stay compressed, and Borg
provides content-defined deduplication and GC. Repositories use `repokey` with an
empty passphrase because these are public product artifacts, not private account
data. Export the Borg keys if protecting against repository-key loss; this setting
does not promise confidentiality. Do not put credentials or user data in these
archives.

## Health and recovery

Pipeline health reads each worker's versioned status under
`DATA_ROOT/health/archive/`. It checks the independently pinned `product-archive-v2`
telemetry contract. Missing, malformed, or mismatched promised telemetry is a
critical coverage alarm. For both collection and storage, lack of a completed
successful pass warns after **15 minutes** and becomes critical after **60
minutes**. A stale status file continues to age on the monitor's clock, so a dead
worker cannot hide an interruption by going silent.

Failure counts, recorded capture gaps, buffer sizes, and repository sizes remain
visible diagnostics, not independent alarms. Recovery clears the progress alarm
automatically, even when a historical gap cannot be filled. Review the retained
gap records when investigating outages; acknowledgement is optional:

```sh
cat /mnt/aerobag-archive/live-buffer/gaps/*.json
python3 /opt/aerobag/tools/archive_service.py --acknowledge-gaps
systemctl status aerobag-archive-collect.service aerobag-archive-store.timer
journalctl -u aerobag-archive-collect -u aerobag-archive-store
```

Acknowledgement clears the reviewed gap markers, not historical gap records
already written into batches. It is not required to clear an alarm and does not
repair missing data.

Status includes `running`, `deferred`, `succeeded`, or `failed`, plus the current
phase. Running and deferred passes preserve any previous genuine failure until
a complete successful pass supersedes it. The durable progress reference is the
last successful pass, or the first tracked attempt if none has succeeded yet;
deferrals and restarts cannot reset it. Pipeline health computes its age using
the monitor's clock and uses the same 15/60-minute policy for both workers.
Thus frequent failure, running, or deferral reports cannot hide starvation.
Unchanged publications count as successful checks, so quiet product periods do
not cause an alarm. An isolated failed pass below the warning threshold remains
available in diagnostics without paging the operator.

This is deployment tooling and telemetry, not a client or NAVDB contract
change. After integration, the operator's `tools/prod_manage.py --reconcile`
installs it without staging a new app or rebuilding products. Collection
restarts during the runtime handoff, before monitoring restarts and before any
lengthy product builds. The store adopts the new code on its next invocation,
without forcibly interrupting an in-flight archive. Until both workers emit v2
status, the upgraded monitor may briefly report a contract mismatch; the
collector's restart no longer waits for product reconciliation to finish.

List and extract a retained day (replace `live-DAY-ID` with an actual name):

```sh
export BORG_PASSPHRASE=''
export BORG_CACHE_DIR=/mnt/aerobag-archive/borg-cache
export BORG_SECURITY_DIR=/mnt/aerobag-archive/borg-security
borg list /mnt/aerobag-archive/live-feeds.borg
mkdir -p /tmp/recovered-day
cd /tmp/recovered-day
borg extract /mnt/aerobag-archive/live-feeds.borg::live-DAY-ID
```

Select a product's last baseline/replacement and its ordered delta suffix at a
particular time, using only that extracted day:

```sh
python3 /opt/aerobag/tools/archive_service.py \
  --export-live /tmp/recovered-day --product nexrad \
  --at 2026-10-09T18:00:00Z --output /tmp/recovered-nexrad
```

The output `timeline.json` lists the copied payloads and any captured gaps.
For NEXRAD it selects the latest complete frame, including all PNG tiles.
For delta products it exports an independent baseline and the ordered original
deltas; it does not reinterpret them through a second implementation of feed
semantics. Cycle archives extract analogously from `cycles.borg`, with the saved
current manifest and its `published/` tree.

## Regression coverage

`tools/test_product_archive.py`, `tools/test_live_feed_archive.py`, and
`tools/test_archive_service.py` cover midnight classification, publication during
baseline capture, actual process exit at four commit boundaries, pointer-replace
failure after rename, orphan cleanup, idempotent retry, retirement, hash mismatch,
expired chains, equal-timestamp full frames, quotas, and independent restore.

Real Borg integration tests archive/extract/delete/compact actual repositories,
including shared chunks, failure after create, and another rotation during
verification. Hosted Python CI installs Borg so these tests execute there.
`tools/test_archive_sandbox.py` also boots the generated systemd units against
isolated synthetic artifacts and a private archive mount. It checks temporary
space, collector read-only boundaries, real hardlinks surviving publication GC,
both Borg repositories' create/verify/restore paths, recovery of an incomplete
pin, and unchanged-publication idempotence. It never uses production paths or
units. Hosted Python CI runs this test explicitly as root and fails if its
prerequisites are missing; ordinary non-root discovery reports it as skipped.
Local root discovery runs it automatically when systemd and Borg are available.
Rust helper tests prove NOTAM checkpoint/suffix replay, updates/removals, and
rejection of missing or reordered deltas. Monitor tests cover missing promises,
bad claims, stale workers, persistent gaps, and explicit disablement.
Cycle-lock regressions exercise actual `flock` contention, source GC immediately
after pinning but during flush, failed flush/retry, ready-snapshot reuse while
the controller is busy, store-worker exclusion through Borg, and deferred
retry without false success. Monitoring tests ensure running/deferred status
cannot hide real failures or prolonged lack of completion.

```sh
python3 -m unittest discover -s tools -p 'test_*archive*.py'
python3 -m unittest discover -s product/preprocessor/scripts -p 'test_archive_health.py'
sudo env AEROBAG_REQUIRE_ARCHIVE_SANDBOX_TEST=1 /usr/bin/python3 -m pytest tools/test_archive_sandbox.py
cargo test --manifest-path crates/notam-state/Cargo.toml --bin aerobag-archive-notam
```
