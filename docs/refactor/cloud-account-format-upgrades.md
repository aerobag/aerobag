# Explicit Cloud Account Format Upgrades

## Status

Implemented. This plan implements client-enforced format compatibility and
explicit, atomic account upgrades. ACS remains an opaque encrypted store;
its existing root compare-and-swap provides atomicity, not schema enforcement.

## Contract

- The encrypted root's existing `version` is the account data-format declaration.
  Format 1 is the current account format (including flight-plan record schema 3).
  Account format numbers are distinct from application versions and individual
  record schema numbers. Do not renumber unchanged data merely to match examples.
- Decode the stable version header before decoding the version-specific root
  body or fetching its referenced page. Unknown formats are compatibility states,
  not attempts to decode a current structure with missing-field fallbacks.
- A format identifies the root/page/record contract as a whole. Future changes
  to stored meaning require a new format and a registered, explicit migration.
  Internal-only refactors do not automatically require a new stored format.
- Normal sync reads and writes only the exact format required by the client.
  A newer client does not automatically migrate or publish an older format.
- Every normal publication must be based on a root revision whose format this
  running client verified. Restarting, linking, and losing CAS require a fresh
  check. Never reuse an unverified persisted root or merely substitute a newer
  revision number after CAS fails.
- Upgrades migrate the authoritative cloud snapshot, preserve record mutation
  stamps and unrelated records, and atomically publish the new tree and format
  declaration with CAS against that snapshot. Migration itself is not a user
  flight-plan edit. Pending local edits remain local until ordinary reconciliation.
- A losing upgrader rereads the winner. If the account is still at the consented
  source format, migrate its latest data and retry. If another upgrader already
  reached the required format, reconcile it normally. Never downgrade or proceed
  to an unconsented format. Interrupted upgrades require explicit confirmation
  again unless the new root was already committed.
- Migration/encoding failures publish nothing and retain local edits. A missing
  migration path is explained; do not guess a conversion or drop fields.

## Core-Owned UX

- Account older than this client: persistent caution, normal sync paused, Cloud
  page explains the account/client data-format numbers and offers Upgrade account
  only when a complete migration path exists.
- Account newer than this client: persistent caution, normal sync paused, Cloud
  page asks the user to update Aerobag (reload the tab on web).
- Upgrade account opens the existing core-owned confirmation experience. Show
  source/target formats, explain that older clients must update, and state that
  the change cannot be undone. Cancel/back has no cloud writes.
- No background modal, no upgrade on reload, link, Sync now, or ordinary edits.
  Local flight planning/navigation and durable pending edits continue while paused.
- Compatibility checks continue while paused, so another instance's upgrade and
  a local client update can restore sync. Compatibility is not a persisted
  permanent-error latch. Connecting to a differently versioned account must
  still leave a usable Cloud page, including recovery/unlink actions.
- Core owns state, wording, enabled reasons, and commands; platforms render the
  existing Cloud panels/confirmation and execute only platform effects.

## Implementation Order

1. Add root-format parsing, format policy/migration ownership, and failing core
   protocol tests. Keep future-format test support inaccessible in normal builds.
2. Gate every publication and CAS retry; implement paused compatibility states,
   read-only recovery polling, and explicit upgrade transactions.
3. Project the Cloud-page caution/confirmation controls and shared caution status;
   generate wire contracts and adapt generic platform renderers only as needed.
4. Add deterministic multi-instance core tests and real shared UI journeys. Use
   the existing in-memory ACS transport and hermetic journey cloud server, not
   production accounts or credentials.
5. Run targeted regressions, both claimed UI platforms, and the shared cheap
   preflight. Record results and any unrun checks here before handoff.

## Test Matrix

### Protocol and Persistence

- Current-format creation and ordinary crossfill still work.
- Both mismatch directions pause without adopting data or writing any objects.
- Future headers with an unknown body yield a useful pause before body decoding.
- Missing/malformed headers and corrupt encrypted data remain integrity failures.
- Upgrade requires confirmation; cancel, reload, edit and Sync now cannot authorize it.
- An older client returning from offline cannot overwrite an upgraded root;
  exercise both CAS orderings, including an in-flight stale publication.
- An upgrader losing to a normal edit migrates the latest committed data, not
  its original snapshot. Two simultaneous upgrades converge without downgrade.
- A migration changes real synthetic fixture data, preserves unrelated records
  and timestamps, and remains atomic when conversion fails.
- Pending local edits and active-navigation adoption rules survive pause,
  restart, upgrade, and reconciliation. Cached old-format records cannot leak
  into new-format output after an application update.
- After restarting with compatible client code, sync resumes without clearing
  app data or pressing a hidden recovery button.
- An interrupted upgrade before/after root CAS has deterministic recovery.

### Multi-Instance UX

- Two instances initially share an account; upgrading only one client's format
  pauses that instance and leaves the older instance usable.
- The newer instance displays the caution and Upgrade account action; opening
  its confirmation and cancelling leaves the account and peer unchanged.
- Confirming upgrades the shared account. The newer instance resumes; the older
  instance displays the client-update caution and preserves offline/local edits.
- Updating/reloading the older instance into matching client code clears the
  pause automatically and restores crossfill.
- Two newer instances observing the same older account: one confirms; the other
  resumes from the event/poll without redundant confirmation or another migration.
- Exercise existing Cloud controls through actual rendered web and Android UI,
  including navigation away/back, enabled states, confirmation/cancel, and the
  shared caution indicator. Do not substitute source assertions or mocked clicks.

## Verification

- Core protocol tests are in `cloud/tests/account_formats.rs`; existing creation,
  crossfill, encryption, and record tests remain in `cloud.rs`. The mismatch and
  restart-write tests were observed failing before implementation. Additional
  red/green checks cover failed-migration feedback, page-verification bypass,
  and a notification arriving during an older root read.
- `cargo test --manifest-path ui/core-rust/Cargo.toml -p app-core --lib cloud::tests`
  passed all 38 tests. They exercise both format directions, both CAS orderings, simultaneous upgrades,
  interrupted/restarted clients, deferred navigation adoption, preserved edits,
  genuine data migration, and immutable format-1 round-trip expectations.
- `shared.cloud-account-upgrade` passed with three real browser instances and
  with an isolated Android emulator plus two browser peers. Both run against a
  disposable real ACS server and the pinned NAV25 compact publication. The
  journey is registered for normal shared P1 qualification, not manual-only.
- Journey screenshots and reports from development are under
  `/tmp/aerobag-cloud-format-results/shared.cloud-account-upgrade/` (web in
  its `web/` subdirectory). They include the paused state, confirmation, and
  older-client caution. These are development artifacts, not required fixtures.
- The UI runs exposed two harness issues: a hand-copied Android action allowlist
  and synchronous Android observation waits starving browser-peer HTTP I/O.
  Actions now derive from the generated core schema. A deterministic scheduler
  test demonstrates the starvation before the fix; the observer now yields to
  I/O without increasing deadlines or sleeping for product readiness. The cloud
  action-revision marker now belongs to the scroll viewport, not offscreen text.
- Full inexpensive qualification command: `/usr/bin/python3 tools/ci/cheap_preflight.py`.
  All 13 suites passed, including 340 harness tests, core/other Rust suites, generated contracts,
  Android JVM, web, Python, licensing, and static checks. Hosted CI and unrelated
  fixture replay/release journeys are separate and were not claimed by this task.

## Adding A Real Successor

1. Preserve the previous descriptor and its root/page/record decoders. Register
   a new `AccountFormat` with the prior descriptor as its predecessor. Do not
   reinterpret old records with the new application's types or change a global
   record schema constant underneath the old decoder. Internal model refactors
   must preserve the stored shape through explicit conversion; changes to stored
   meaning need the new descriptor and migration instead.
2. Implement and test the cloud-record migration and page encoder. Migration
   retains existing keys and their mutation stamps, including unknown unrelated
   records. A future key-renaming/deletion migration needs an explicit extension
   of that invariant, not deletion of the guard.
3. Keep the stable root header readable. Change the client's current descriptor;
   keep the historical local-settings default at format 1. Application version,
   account format, and individual record schema numbers are different contracts.
4. Add frozen old/new data tests and run the existing multi-instance race and UI
   suites. Record meaningful payload changes, not just relabeled version fields.

The hermetic successor used by tests is format 2 with a different page shape
and a synthetic record migration. It is available only under Rust tests or
`cloud-format-test`, which app build scripts enable only for E2E builds.
Ordinary clients require format 1 and do not offer this fake upgrade. The
journey simulates a binary update by changing only a test-build selector while
the old app is stopped; all account reads, consent, encryption, migrations,
CAS, and UI transitions use production paths. Production accounts and device
credentials are never used by these journeys.
