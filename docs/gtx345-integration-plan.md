# GTX 345 Receiver Integration

## Agreed Contract

- Android supplies Classic Bluetooth RFCOMM, permissions, paired-device
  discovery, private file IO, and monotonic/wall clocks. Rust core owns the
  protocol, subscriptions, decoding, source selection, status, and UI actions.
- Use the established mutual authentication unchanged. Credentials and flight
  captures remain private, outside source control. Send no transponder
  configuration, firmware, or calibration commands.
- Live input and replay use the same bounded incremental decoder. Capture raw
  RX/TX before interpretation, including unknown and malformed messages, with
  clock context. Capture must not depend on successful rendering or decoding.
- Maintain independent internet and receiver weather stores. Never modify the
  internet full/delta base with receiver observations.
- A shared spatial station directory holds identity/location and provenance,
  not selected weather. Cycle metadata and live feeds may introduce stations;
  stations absent from NAVDB remain first-class. Cache learned locations with
  weather for offline restart. Report freshness does not select coordinates.
- All weather consumers use one core query API. It compares per-source reports
  on demand using observation/issue time, with explicit correction/validity
  handling. Reception time must not manufacture freshness.
- A future reported timestamp is timing-quality information, not a reason to
  hide readable weather. Preserve the timestamp, expose its signed relationship
  to the current clock and any discrepancy at receipt, and never gate report
  visibility on the clock catching up. Calendar reconstruction is separate from
  freshness; do not rewrite a near-future date as last month or as receipt time.
- NEXRAD source is explicitly Internet or ADS-B, never blended. One animation
  policy runs over the selected history. Preserve source palettes and distinguish
  no coverage from no precipitation. The grid shows source and report age; the
  existing tiny-tray mechanism exposes source and animation controls.
- BARO source is explicitly selected using the same tray mechanism. Never
  average aircraft static and tablet cabin pressure or silently switch sources.
  Pressure, pressure altitude, and geometric altitude are distinct quantities.
- Record/inspect AHRS; an attitude instrument is outside this project.
- Retain Airplanes.live code but remove its debug activation entry.
- Production archival is a separate project. Radar placement remains provisional
  pending comparison using a future capture and independent radar imagery.

## Implementation Order

1. Shared receiver protocol/decoder, bounded reassembly, private corpus replay,
   and synthetic malformed/order/split-input regression tests.
2. Shared station directory, separate report stores, one query interface, and
   migration of every existing METAR/TAF consumer. Test feed-only stations,
   report ordering, disconnection, persistence/restart, and expiry.
3. Receive-only protocol controller with supported-ID-filtered subscriptions,
   unchanged mutual authentication, bounded transport state, and diagnostics.
4. Android RFCOMM/private capture plumbing and core-owned connection UI. Verify
   recording/export, failure handling, lifecycle, and replay without avionics.
5. Traffic/ownship and verified pressure inputs; source-selected radar history
   and tiny trays on both platforms. Exercise existing shared map bindings.
6. Full offline corpus validation, build Android artifact, then independent
   parked validation before the next flight.

## Evidence And Limits

The private October 6 handoff is the protocol reference. Its independent parked
client received attitude, pressure, and traffic; nonempty weather is demonstrated
only by the Pilot recording. Standalone pressure-message units, some traffic
qualifiers, metadata flags, and negotiation/retransmission details are unresolved.
Unknown fields remain explicit, and unsupported products remain captured rather
than guessed. Do not describe offline replay as live validation.

## Progress

- [x] Bounded shared decoder and private corpus replay
- [x] Shared weather query/directory foundation and local cache format
- [x] Migrate existing weather consumers and source ingestion to that boundary
- [x] Protocol controller and reconnect handling
- [x] Android transport and durable automatic capture (application wiring; hardware verification pending)
- [x] Core-owned source controls and platform rendering
- [x] Offline integration validation and Android release build (not hardware validation)
- [ ] Independent parked connection (requires hardware)

### Implemented Foundation (October 9)

The weather query boundary and native receiver worker are now connected to the
app. Android Settings has a core-owned experimental receiver panel; independent
hardware validation is still pending. The
Airplanes.live activation checkbox is removed; its persisted flag is also
ignored, so restoring preferences cannot restart that experiment.

- `app-core/src/receiver/` owns incremental framing/reassembly, bounded product
  decoding, supported-ID subscriptions and the unchanged mutual authentication.
  The connection protocol has bounded outstanding writes/retransmission and
  setup deadlines; it does not disconnect a receiving session just for quiet
  weather.
- `receiver/capture.rs` defines private append-only binary capture records with
  RX/TX/event kinds, monotonic and wall clocks, sequence, and checksum. The
  generic sink exposes write/flush/durable-sync, not aviation concepts. Recovery
  preserves a complete prefix after a torn final record, and rejects corruption
  of complete records. Write-request and write-completion records are distinct:
  a partial/failed socket write is not claimed as a successful transmission.
- `weather_sources.rs` separates station geography from observations, including
  feed-only stations and persisted learned locations. Its only report read API
  selects between independent stores. Authoritative source replacement is
  transactional and source-local; receiver coverage updates are incremental.
  Per-report, count, aggregate text and snapshot bounds limit memory.
- Internet METAR/TAF installation now normalizes into the separate Internet
  store, retaining publication metadata independently from selected reports.
  Map rendering, inspection, WX detail, FP badges, and nearby altimeter lookup
  all consume the shared query. Map and inspection share spatial candidate
  enumeration; no consumer scans the national report store. The old runtime
  METAR tile-index builders are removed; PIREPs retain their existing index.
- A scheduled core session entry accepts normalized receiver reports and
  invalidates map/FP projections only on changes. Synthetic tests exercise the
  real session update and FP action, not just the standalone report selector.
- Flight-category/cloud symbols in Internet products come from structured
  upstream fields. There is no existing local raw-METAR grammar to reuse.
  Receiver text currently has unknown classification: a newer receiver report
  must not borrow an older Internet report's VFR/ceiling symbols.
- Cycle airport-alias coordinates are airport ARPs, not exact weather-station
  locations. They are not used to seed the directory. An exact cycle station
  catalog, wiring the local weather-cache envelope into platform persistence,
  and receiver classification remain followups. Reports with no known station
  location are readable by ID, but are not falsely placed on the map.
- Synthetic tests cover fragmentation, corruption, authentication ordering,
  unsupported subscriptions, duplicate delivery, sequence gaps, retry bounds,
  sensor validity, decompression limits, UTC month rollover, report selection,
  source isolation, dateline geography, offline cache restart, and torn capture
  recovery. No private credentials or captured flight records are test fixtures.

### Offline Evidence

`app-core/examples/receiver_replay.rs` accepts the private archive's
`captures/flight-frames.jsonl`. Run it using:

```sh
cargo run --manifest-path ui/core-rust/Cargo.toml -p app-core \
  --example receiver_replay -- /private/path/captures/flight-frames.jsonl
```

It reconstructs transport frames, splits input into small reads, compares every
completed Rust application message to the archive extractor, decodes products,
normalizes station weather, and checks weather-cache restart. Output contains
counts/rejection reasons only. Reconstructed frame checksums are not independent
validation of original HCI checksums; those belong to the archive extractor.
This is not a replay of the active authentication/retransmission controller or
an end-to-end application rendering test.

The captured corpus passes product decoding/reassembly: 1,493 traffic snapshots,
7,445 attitude and 7,445 pressure samples, 16,983 checksummed NMEA sentences,
33 nonempty text-weather snapshots and 11 radar snapshots. All 613 TAFs and
1,416 METARs normalize. Two METARs have observation times approximately 22 and
198 seconds after their GPS-anchored receipt time. They remain available
immediately, with original timestamps and explicit timing-discrepancy metadata.
The initial rejection policy was removed; regressions proved both the ingestion
rejection and query-time hiding before the change. Tests cover cache restart,
clock rollback, and month/year rollover without changing the source timestamp.
This discrepancy alone does not establish that the observing site's clock was
wrong. Replay reports timing anomalies separately from normalization rejection
counts and binary decoder errors. Date calibration uses the archive's valid RMC
time, not its incorrect HCI wall clock.

Native core tests, WebAssembly compilation and the 13-suite cheap preflight have
passed for the foundation. These are not claims of Android transport, full app
build, journey, or independent live-device validation.

### Next Integration Boundary

`receiver/connection.rs` now owns connection generations, connect/write deadlines,
bounded queued transmissions, checkpoint timing, and reconnect backoff (5 seconds
doubling to 65 seconds, reset only after a minute of receiving). An old socket's
late data/error/completion cannot affect a replacement connection. Quiet weather
does not trigger a disconnect. Storage failures are latched: stop the connection,
preserve the durable prefix, and require a freshly opened recorder. A quota fault
has a distinct status asking the user to export/remove captures. No automatic
retry can make a failed capture appear healthy.

`receiver/capture_files.rs` supplies a native private archive: 16 MiB immutable
segments, a 256 MiB total cap, and a 512-segment cap, including previous runs.
There is no silent evidence eviction. A kernel file lock excludes another owner;
directory sync makes newly created segments durable. The connection checkpoints
at least once per second or after 256 KiB of recorded data, plus at disconnect.
Sealing for export returns immutable segment paths without changing old bytes.
Torn old segments are preserved, never reopened for append.

Android's `AndroidRfcommTransport.kt` is OS-only plumbing: paired-device inventory,
permission facts, a selected device/service socket, bounded reads, one outstanding
write, and cancellation via socket close. It owns no retry, timeout, aviation
decoder, source policy, or UI labels. Threaded fake-stream tests exercise blocked
connect/read/write, immediate failures, exact write completions, bounded queues,
and cancellation using explicit gates rather than sleeps. Native core tests drive
a synthetic peer through the real handshake and inject capture/transport faults.

The native bridge now connects these components to Android's process-lifetime
receiver worker and a connected-device foreground service. While explicitly
connected, the service holds a partial wake lock so screen sleep does not suspend
recording, and releases it on stop. Settings receives a typed panel from core.
Kotlin renders labels and forwards opaque action IDs;
generated contracts carry Bluetooth/file effects and completions. Permission and
document selection are OS plumbing. Core owns selection, stale action rejection,
credential validation/storage, retries, status and export contents.

Credentials are imported using the two document-picker actions (`auth-token.bin`
and `auth-user.bin`). Core bounds and validates them before atomically storing
private copies beneath `files/receiver/`. They are never compiled into the APK,
persisted in normal preferences, or cloud-synchronized. Pairing the device remains
an Android system-settings operation. No connection starts without an explicit
device selection.

The capture worker has no UI session reference. Its bounded mailbox coalesces
already-committed weather changes by station/product, leases at
most 32 reports per delivery, and retains that lease until the scheduled session
mutation succeeds (including resource paging). Status publication is limited to
one signal per second by core. Native reports stay native rather than crossing
Kotlin as a national JSON product. The Settings status and weather projections
land through the same existing incremental session-update machinery.

Disconnect releases the socket and closes/checkpoints the capture. Export then
streams immutable segments into a private ZIP and invokes Android sharing with
a URI grant restricted to the exports directory. It never deletes evidence;
an operator must archive/remove old segments when the 256 MiB quota is reached.
Only one export ZIP is retained locally. The ZIP excludes credential *files*,
but raw captures include protocol authentication exchanges: captures/exports
are sensitive and must not enter source control or public artifact serving.

### Receiver Weather Durability (October 10)

Core now owns a separate private `files/receiver/weather.sqlite` latest-report
cache. It keeps one METAR and one TAF per station, not an ever-growing history;
raw evidence remains in the capture archive. A kernel lock excludes competing
cache owners. SQLite FULL synchronous rollback-journal transactions commit each
received weather batch before making it eligible for UI delivery. No national
Internet product is copied or serialized for these writes, and no disk IO runs
under the UI session or mailbox lock. The report index's existing count/text
bounds apply; the SQLite file also has a 64 MiB ceiling independent of page size.

The report index prepares changed rows transactionally using the same precedence
and validation for both sources. It does not mutate until commit; the native
cache commits disk first. Session ingestion also applies a whole delivery
atomically. The mailbox makes no second freshness decision. Cache failure is
latched and shown in receiver status, suppresses publication of new undurable
weather, and does not stop independent raw capture. Unknown/corrupt cache formats
are reported, never silently discarded.

A newly attached UI session receives all cached receiver reports in bounded
32-report deliveries even after an earlier session acknowledged them. Consumer
generations (allocated at publisher construction, not asynchronous attachment)
and delivery leases reject stale readers, late attachments and acknowledgements. Process
restart restores report and receipt timestamps unchanged, including future-dated
observations; restoring weather does not restore live sensor samples or reconnect
Bluetooth. Tests exercise disk restart, competing owners, transaction rollback,
session replacement, changes during an outstanding lease, actual SQLite-full
errors, corrupt caches and continued raw-capture export. A core-session test
reopens the on-disk cache and invokes the airport FP row's WX action in two
successive sessions, checking displayed report text and age without any network
or Bluetooth connection. Learned-station-directory persistence remains a
separate followup; existing durable Internet products can reseed that directory.

At that checkpoint, remaining work was explicit BARO/NEXRAD selection, MSL input,
independent parked validation, and full handset integration/journey coverage. Rotation uses the
retained session and does not reset the worker. A process restart deliberately
does not automatically reconnect. Browser Classic RFCOMM is not implemented;
web does not receive the native receiver Settings block.

Component checks cover private credential import and stale picker results,
real archive/export identity, socket callbacks after disconnect, bounded/retried
session deliveries while capture continues, and physical taps on the Settings
receiver controls. The existing session test now enters via the production
receiver delivery API and proves that receiver status, FP WX actions and Internet
TAF preservation update together. These are not independent hardware validation.
Clock samples must be taken inside the serialized connection owner (not before a
callback queues); file IO/decoding must never hold the UI session mutex. Only after
the integrated path is exercised offline should the first parked test be requested.

### Live Traffic And Ownship (October 10)

The background owner now publishes a coalesced, immutable live snapshot alongside
weather deliveries. Sensor state is volatile, never restored from the weather
database. Disconnect clears it; reconnect starts empty. Leases retain original
receipt/event times, and delivery supplies current wall and monotonic clocks so
waiting behind resource paging cannot manufacture a fresh GPS fix. A frozen
receiver GPS timestamp expires even while new traffic packets continue arriving.
Receiver fixes expire after ten seconds. Traffic includes upstream snapshot and
target age and uses the shared fifteen-second display limit. Monotonic deadlines
also clear the live state if the wall clock stops or moves backward.

After a receiver connects, core exposes ADS-B Traffic in Layers and Receiver in
the ownship menu. Neither is selected automatically. Receiver GPS uses the normal
ownship sampling, guidance sequencing, map follow, freshness, and incremental UI
projection paths. Losing GPS preserves source/follow intent and the last viewport;
the launcher says `Receiver: No GPS`. Reconnecting does not resurrect an old fix.
The shared source expiry deadline is no longer overwritten by the hidden
Internet ADS-B polling deadline.

Traffic map projection and inspection share a single iterator for visibility,
age, position and like-datum altitude separation. Inspection no longer performs
a second linear lookup for every visible target. Both platforms render the
core-supplied outline: a non-directional diamond when track is unknown, rather
than pretending unknown means north. Receiver track identity includes connection
generation, receiver track ID and address qualifier; an unqualified address or
callsign is not treated as an Internet registration. Receiver traffic cannot
start Airplanes.live polling.

Enhanced traffic pressure altitude is retained as pressure altitude. Geometric
altitude is not presented as MSL. Receiver direction/vertical-velocity qualifiers
are still unresolved and are not guessed. MSL-tagged NMEA fusion, BARO source
selection, and source-selected NEXRAD are described in the next checkpoint.

Synthetic core-session tests exercise delivered map/source menu patches,
receiver-driven flight-plan sequencing and its UI patches, traffic rendering and
expiry without any Internet request, delayed delivery, stale/invalid GPS,
disconnect/reconnect, and preserving CTR through signal loss. Mailbox tests
exercise changing sensor snapshots while weather pages and old leases retry.
Private corpus replay feeds the same live reducer: all 1,157 snapshots with valid
ownship coordinates produced a current fix, out of 1,493 traffic snapshots. This
is offline evidence, not independent hardware validation.

### MSL, Pressure Selection, And Receiver Radar (October 10)

Core joins checksummed GGA MSL height to enhanced-traffic GPS only for the same
UTC second and positions within 0.05 nm. Either arrival order is accepted;
invalid/estimated/simulator GGA fixes cannot supply navigation altitude. The
join never extends the fix's original freshness deadline. No height is borrowed
from a different second or from the geometric-altitude field. Calendar recovery
handles UTC midnight; receipt-age checks reject stale sentences. GGA field
semantics are documented in the receiver handoff and
[the NMEA GGA reference](https://receiverhelp.trimble.com/oem-gnss/nmea0183-messages-gga.html).

The BARO tiny tray now offers DEVICE and RECEIVER. Each source has its own
bounded filter/history; changing source cannot mix readings or vertical-speed
samples. The default remains device pressure. Receiver pressure uses only the
verified enhanced-traffic pressure-altitude field, converted through the inverse
of the existing standard-atmosphere equation before applying the selected
altimeter setting. It works without selecting receiver GPS or having a device
barometer. Loss of the selected source produces no reading, not a switch to
another source. The standalone unverified pressure message is still capture-only.

The NEXRAD grid opens the same tiny tray, with INTERNET/ADS-B and ANIMATE/LATEST
choices. Core sends all labels, selected states, action IDs, and whether the tray
has a text input. Both platforms render that contract; Android does not summon
a keyboard for action-only trays. The cell identifies NET or ADSB and uses the
same selected-frame age as the map animation. Internet live-feed status continues
to describe Internet data even when the map is displaying receiver radar.

Receiver radar is converted into 256-pixel PNG tiles on the ingest worker,
not under the UI session lock. Immutable tiles/history are shared by Arc. The
history uses the existing frame-count policy plus a 32 MiB compressed-byte bound;
only a tile-sized RGBA buffer is encoded at a time. CONUS lies below regional
radar in each receiver frame, preserving source palettes and no-coverage shading.
Unknown codes shade as no data, not clear weather. The grid age conservatively
uses the oldest displayed layer; expired layers are removed independently.
Disconnect retains radar with its original dates. The followup below also makes
that bounded history survive process restart, independently of the raw recording.

Both sources use one source-grid mesh projector and animation policy. Core-owned
image URIs use the existing tile-byte bridge on Android and a WASM byte bridge on
web; browser object URLs use the existing frame-cache lifecycle. Missing local
images fail locally, never turn into HTTP requests. Internet eager/durable vs
viewport acquisition preferences are unchanged. Receiver radar's placement
remains explicitly experimental in the tray pending an independent comparison.

Tests exercise source controls through a real core session under both platform
acquisition policies, receiver PNG bytes and no-data pixels, map/grid animation
agreement, disappearance after expiry, disconnect retention, no network fallback,
GGA ordering/invalidity/midnight, and pressure-source isolation. Web component
tests and Android physical-tap tests use the shared production tray. Private
corpus validation prepares all 11 radar snapshots, preserves all 1,157 current
GPS fixes, and exercises MSL fusion without decoder or normalization failures.

### First Parked Validation

Use the release preview APK built from this work; do not treat synthetic replay
or component tests as independent avionics validation. No credentials are built
into the APK. No Bluetooth connection starts automatically on process restart.

1. Pair the tablet and GTX 345 in Android system settings while parked.
2. In Settings / experimental receiver, import the private `auth-token.bin` and
   `auth-user.bin` files, grant Bluetooth permission, then explicitly connect to
   the paired receiver. Capture starts before interpretation.
3. Confirm receiving/message/capture counters advance. Select Receiver for
   ownship, enable ADS-B Traffic, and explicitly choose receiver pressure/radar
   in their grid trays if desired. Compare GPS/MSL and pressure with the panel;
   do not infer verified standalone-pressure units from a plausible number.
4. Disconnect and export the sealed private capture. Treat the export as secret:
   raw protocol authentication exchanges are included. Preserve it outside Git
   and public artifact serving. The capture quota is 256 MiB; reaching it stops
   the connection rather than silently discarding evidence.
5. Replay the capture for protocol ordering and data validation, then compare a
   future radar capture against independent archived government imagery. Live
   nonempty weather reception and radar geographic accuracy are not yet proven
   by an independent Aerobag connection.

For the ZIP exported by the application (not the older Pilot JSONL):

```sh
cargo run --manifest-path ui/core-rust/Cargo.toml -p app-core \
  --example receiver_capture_replay -- /private/path/receiver-capture.zip
```

Replay streams bounded ZIP entries without extracting files. It checks capture
checksums, numbered segment continuity, monotonic clocks, actual transport
checksums, and application reassembly. Retransmission/sequence checking is shared
with the live protocol. Random capture-session IDs are not chronological; each
session resets live sensor clocks. A torn final record preserves the complete
prefix and is reported, while corrupt complete records, missing segments, and
data after a torn segment fail. Requested and completed writes have distinct
counts. Output contains aggregate diagnostics only, not credentials, locations,
or weather bodies. Passive replay cannot prove mutual authentication succeeded.
Tests cover the production runtime's actual export in addition to fragmented,
retransmitted, reconnected, truncated, corrupted, and out-of-order input.

The October 10 followup below adds exact cycle station metadata, raw-METAR
classification, and radar restoration. Independent hardware validation remains
required. Reports without known station geography remain readable by identity.

### Validation And Next Boundary

The October 10 tree passes the complete 13-suite cheap preflight, including core,
web, Android JVM/physical-tap components, generated contracts and licensing.
The receiver-focused suite has 59 passing tests. The actual exported-capture CLI
also passes a fragmented/retransmitted synthetic NMEA recording. The private
Pilot corpus has no decoder or normalization failures. Its aggregate replay took
3.33 seconds and 6.1 MiB maximum RSS on the dev host; that is a decoder/replay
measurement, not the Android application's memory usage or a tablet benchmark.
The arm64 release APK and optimized web/WASM build both succeed. The APK uses
the dev stack and the normal signing key/package name. No tablet installation or
independent parked GTX connection has been performed.

The shared NEXRAD animation journey now uses the visible source/animation tray
instead of the removed single-tap toggle. It proves selected mode AND painted
frame, holds the newest frame through an animation dwell, and resumes. It passes
on the built browser and Android emulator (8.3 and 13.4 seconds for the journey,
excluding build/install/package preparation). The first Android run exposed
unqualified control IDs in the shared grid editor: the tray opened, but its
controls were inaccessible to the indexed driver. A rendered-window regression
failed before the fix and passes after it, including unmount/remount. The same
fix covers BARO and target-altitude controls; all nine shared-editor component
tests pass. The journey retains actual observed mode/frame on failure.

An additional Android Settings smoke check exposed an actual native storage
failure: Rust 1.94.1's `File::try_lock` does not support Android. Both private
weather and capture stores now use the existing repository dependency `fs2`
for native locks, retaining underlying IO errors rather than reporting every
failure as another owner. Cross-compiling the real Rust restart test and running
it on Android reproduced the old failure; all 59 receiver tests pass natively
on Android after the change, including competing owners, capture/export,
SQLite rollback and session restart. Host-only Rust and Robolectric tests cannot
establish native filesystem support; this check must run on the Android target.
The rebuilt APK's Settings panel was also inspected on the emulator: the private
SQLite cache is created and the storage failure is gone. Both NEXRAD journeys
pass again with that build. These checks do not simulate a Bluetooth radio or
establish independent live weather reception.

### Shared Local Storage Followthrough

The receiver work was checkpointed separately, the existing settings/tour
storage was generalized and merged into local main first, and the receiver
branch was rebased onto it. See [the boundary contract](refactor/local-documents.md).

The learned station directory now uses that same keyed opaque-document store on
both platforms. Core owns the document key, schema, validation, metadata revision,
write coalescing and error status. Platforms do not know what stations are.
Only identifiers, exact coordinates and metadata provenance are persisted;
weather reports retain their separate source/cache lifetimes, and the spatial
index is rebuilt on load. One metadata-changing batch queues one replacement;
report-only updates do not serialize or write this document. Restored metadata
is not rewritten at startup. Invalid documents are protected and reported through
core while live station learning remains usable in memory.

The measured dev-feed union had 5,231 stations: directory JSON was 432,946 bytes
(about 423 KiB; 63 KiB gzipped). A three-hour sample introduced 156 stations and
no coordinate changes after the first product; whole-document writes are bounded
and metadata-driven, not per-observation. This cache is local, not user/cloud state.
Exact cycle weather-station metadata still requires a NAV publication/fixture
update; airport ARP aliases remain explicitly unsuitable substitutes.

Real-platform validation with the pinned national weather fixture exposed
out-of-range upstream station coordinates (for example ENUN at -99.99/-99.99).
Report validity and geographic availability must remain separate: both Internet
METAR/TAF adapters retain these reports by station ID but emit no location update.
They neither poison the whole product nor erase an existing learned location.
The shared directory still strictly validates persisted coordinates. Regression
coverage includes both products, preservation across restart, and prepared-product
landing through a real session; the new missing-geography test failed before this
correction. Original failed UI-run evidence is retained under
`/tmp/gtx-local-doc-results` in this development workspace.

### Maximize The Next Hardware Experiment (October 10)

Credentials come from the private October 6 handoff's `reference/auth-token.bin`
and `reference/auth-user.bin`. They are application-level Connext material
recovered from the user's installed Pilot build, not a Garmin account login or
Bluetooth PIN. They worked with the independently tested receiver/firmware; this
does not establish compatibility with every receiver. Durable development copies
are in `/root/aerobag-credentials/gtx345/` (directory 0700, files 0600). Import
both through the existing receiver Settings file pickers. Never embed them in
an APK, a public artifact, Git, or cloud state. Delete any shared-storage transfer
copies from the tablet after import. Exported captures also remain private
because they contain authentication exchanges.

Before installation, complete these software followups and their validation:

1. NAV29 publishes `weather/station-catalog`, containing NASR weather-station
   coordinates, not airport ARPs. Repair the old AWOS fixed-width coordinate
   parser; use the FAA's hemisphere and longitude field width. Core replaces
   cycle-owned geography at adoption while preserving feed-only stations. Rebuild
   real compact fixtures and the matching cycle product before shipping a client
   that requires this contract. Existing NAV28 products cannot satisfy it.
2. Extract the live-feeds cloud-symbol parser into `weather-observation`, shared
   with core receiver normalization. Derive flight category from prevailing
   visibility and observed ceiling, ignoring remarks and forecast trends. Unknown
   fields remain unknown, not VFR; never borrow classification from an older
   Internet report. Re-derive cached display fields from original text on startup
   without changing observation/receipt timestamps.
3. Extend the receiver's existing private SQLite cache to hold compressed radar
   tiles and frame references. Transactionally write new images, replace frame
   history, and reclaim unreferenced images before publishing to sessions. Restore
   on the background owner with original timestamps and frame IDs. Preserve the
   32 MiB radar bound and 64 MiB total database ceiling. A failed durable write
   leaves the previous complete frame available; raw capture continues separately
   with an explicit cache-failure diagnostic. GPS, traffic and pressure are never
   restored as live inputs.
4. Validate thresholds, cross-source selection in all consumers, station catalog
   replacement, offline restart, v1 cache upgrade, quota/failed transactions, and
   private corpus replay. Run the native filesystem tests on Android, not just
   host Rust. Build an arm64 release APK and exercise the visible source controls.

One airport visit should collect more than a connection-success anecdote:

- At home: install the matching app/data, import credentials, warm Internet
  weather, confirm offline packages, and verify recording space. No need to
  discover credential/file-picker problems at the airport.
- Refresh paired devices only rereads Android's bonded-device list; it neither
  scans for nearby devices nor connects. Core shows refresh progress, a UTC
  completion time/count (including unchanged and empty lists), or the specific
  permission/Bluetooth/query failure. A five-second core deadline makes a stuck
  OS query visible; tagged late results cannot restore obsolete connect buttons.
  Pair in Android settings first, refresh, then explicitly choose Connect GTX345.
- Parked: pair/connect explicitly; verify protocol state, raw RX/TX and decoded
  counters, and that capture bytes advance. Record receiver model/firmware. Try a
  short disconnect/reconnect and export/replay before committing to a long flight.
- In normal operations: leave automatic recording running, including unknown
  messages. A passenger can note UTC times when traffic, METAR/TAF and radar first
  arrive; compare selected-source age, receiver GPS/MSL and BARO with the panel.
  Do not make the pilot debug or perform unnecessary avionics operations aloft.
- After landing: inspect source selection, animation and stale/disconnected
  behavior. Restart the app while disconnected and verify reports/radar remain
  readable with their old dates while sensor values remain absent. Export all
  sealed sessions and preserve the ZIP privately. Pair the radar portion with the
  independently collected imagery from the separate archival project.

No simulator/replay result establishes live authentication, nonempty independent
weather reception, or geographic accuracy of the provisional radar decoder.

Software evidence for these followups:

- Rebuilt NASR input produces 2,651 exact station locations, including K1S5
  without requiring an airport record. Producer tests cover missing locations,
  inactive stations, Alaska identifiers and conflicting coordinates.
- Private flight replay accepts all 1,416 METARs and 613 TAFs and prepares all
  11 radar snapshots, with no decoder failures or rejected reports. METAR
  categories: 1,296 VFR, 37 MVFR, 26 IFR, 32 LIFR and 25 unknown. Unknown does
  not hide the raw report or borrow an older source's classification.
- All 63 receiver tests pass as native Android binaries on both the x86 emulator
  and the physical red arm64 tablet (2.44 seconds on the tablet). The tight-SQLite-quota
  regression first reproduced `SQLITE_FULL` when a replacement temporarily
  needed both histories. Eviction now occurs inside the same transaction,
  before allocation. Rollback restores the evicted frame as well as its tiles.
- The external three-hour Internet METAR delta reconstruction still passes.
- Full cycle-2610 product publication passed integrity validation. The isolated
  package source is
  `http://aerobag-dev.iac.jonh.net:18080/packages/gtx345-preview/`; the shared
  dev-stack catalog remains unchanged. Both compact CI publications were rebuilt
  from this product, not relabeled, and pinned together in the artifact lock.
- `shared.prepared-live-feeds` and `shared.nexrad-frames` pass on web and native
  Android against those new fixtures. These prove normal weather consumers and
  source/animation controls still work, not live Bluetooth reception.
- The old flight archive has 11.2 MiB of wire data across 27 active minute
  buckets (the elapsed span includes long gaps); its busiest minute is about
  646 KiB. This suggests useful headroom under the 256 MiB capture limit, not
  a guaranteed recording duration: socket chunk overhead and future receiver
  traffic differ. Quota exhaustion is explicit and never evicts old evidence.
