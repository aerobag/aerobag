# User Cloud Storage Design Plan

## Status

This document defines the intended architecture and first vertical slice for
synchronizing Aerobag user state across devices.

The first provider is Google Drive. Two browser experiments established its
storage semantics:

- Drive file updates do not provide a usable compare-and-swap. `If-Match` with
  the exposed file `version` was ignored for simple, multipart, and resumable
  uploads.
- Drive-generated file IDs do provide atomic create-once. Concurrent creates
  using the same generated ID produced exactly one winner and one
  `409 fileIdInUse` loser.
- Retrying a successful create produced `409`, so an ambiguous result can be
  resolved by reading the reserved ID.
- A generated ID remains consumed after its object is deleted. It is an
  immutable slot, not a reusable lock.

The evidence and lab procedure are in
`docs/experiments/google-drive-cas.md`.

The MVP ends when two independent clients authenticate to the same Google
account, join the same Aerobag cloud account, and a flight-plan edit on either
client appears on the other through the Drive synchronization path.

## Design Decisions

- Cloud use is explicit. Aerobag remains fully local without a cloud account.
- Core owns the synchronized model, encryption, storage shape, merge policy,
  synchronization state machine, retries, polling policy, and UI model.
- Platform code supplies provider-shaped effects, private credential storage,
  generic local persistence, and passive view/controller surfaces.
- The common protocol uses atomic create-once and immutable objects. It does
  not expose or depend on mutable provider writes.
- Every provider uses the same immutable successor-chain algorithm. A provider
  with native mutable CAS may optimize later, but it does not get a separate
  correctness path in the MVP.
- The synchronized application state is an encrypted Merkle tree.
- Normal synchronization follows known object IDs. Listing is for recovery,
  accounting, and garbage collection, not the ordinary read path.
- Notifications are optional latency hints. Polling and chain traversal are
  sufficient for correctness.
- Provider credentials are device-local and are never included in Aerobag
  Device Setup Codes.
- Active navigation state is device-local. A cloud flight-plan definition
  cannot silently replace a plan currently being flown.

## Goals

- Persist selected user state across app reloads and device loss.
- Crossfill selected state between web and Android.
- Keep payloads confidential from providers by default.
- Continue operating locally while offline or while provider authorization is
  unavailable.
- Make remote changes flow through the same core-owned model and invalidation
  path as local changes.
- Define one provider interface that fits Drive, a future Aerobag UCS, and
  providers such as WebDAV when they can prove atomic create-once.
- Surface authentication, synchronization, pending adoption, and errors
  honestly in the Cloud page and Data Status.

## Non-Goals For The MVP

- Synchronizing active-leg execution, direct-to state, ownship, viewport, open
  trays, or other transient navigation state.
- General collaborative editing or user-driven conflict resolution.
- QR scanning; copy/paste Device Setup Codes are sufficient for the first
  slice.
- Google webhook relays or background synchronization after the web access
  token expires.
- Garbage-collecting the immutable successor-chain spine.
- Synchronizing traces, aircraft, preferences, or offline-package policy before
  the flight-plan slice works end to end.
- Aerobag UCS, WebDAV, or provider migration.
- Cloud storage as a package or live-feed cache.

## Identity, Keys, And Device Setup

### Local-First Identity

A fresh installation generates a device ID and stores all state locally. It
does not allocate cloud objects or contact a cloud provider.

The product vocabulary is:

- **Storage provider**: Google Drive, Aerobag Cloud, or another implementation
  of the provider contract.
- **Provider authorization**: this device's credential for one provider.
- **Sync Account**: the encrypted Aerobag account stored at one provider.
- **Linked device**: a device that holds the Sync Account descriptor and can
  verify its genesis state.
- **Device Setup Code**: the secret, copyable descriptor used to add another
  device to a Sync Account.
- **Unlink this device**: after an explicit warning and confirmation, remove
  the local Sync Account descriptor without deleting cloud data or changing
  another device.

Device setup never transfers provider credentials. Each device independently
authorizes the provider named in the Device Setup Code.

Provider authorization is independent of Sync Account setup. `Back` abandons
an incomplete setup journey, and `Unlink this device` removes the local Sync
Account descriptor; neither operation logs out of the provider or revokes an
otherwise-valid provider token. Authorization ends only through expiry,
provider revocation, or a future explicit provider-logout action.

The following invariant is structural, not a UI convention:

```text
Sync Account != NOT LINKED  =>  Provider != NOT SELECTED
```

The provider belongs to the Sync Account descriptor. Core installs both in one
transition when it accepts a Device Setup Code; platform code cannot select a
different provider for an already-linked account.

### Root Secret

Each Aerobag cloud account has a random 256-bit root secret. Core derives
independent material with a versioned KDF and distinct context labels:

- payload encryption keys;
- node and record authentication/signing keys;
- account locator material; and
- recovery/key-wrapping material.

Cryptography and serialization are implemented once in core. Web and Android
must not implement parallel cryptographic formats.

The Device Setup Code contains the root secret, provider kind,
provider-specific non-secret configuration, genesis object ID, contract
versions, and an expected provider-account identity fingerprint plus a display
hint. Possession of the code grants complete account access. It must not appear
in a normal URL query or path.

The identity binding catches the common error where the receiving device
authorizes a different Google account. It is an early diagnostic, not the
security boundary: successful authenticated decryption of the genesis object
is the final proof that the provider account and Sync Account agree.

### Optional Provider Recovery

By default, the root secret exists only on linked devices. Losing all linked
devices loses access to encrypted cloud data.

A future opt-in `Google account recovery` option may store a recovery bundle in
Drive `appDataFolder`. The bundle must contain enough versioned material to
reconstruct the account, including the root secret and genesis ID, rather than
only one derived encryption key.

This deliberately changes the threat model:

- Google, or anyone who can authorize Aerobag against that Google account, can
  recover the synchronized Aerobag data;
- removing the recovery bundle does not undo prior exposure;
- restoring zero-knowledge status requires key rotation and re-encryption; and
- it protects against device loss, not loss of the Google account.

This recovery option is not required for the flight-plan MVP.

## Provider Interface

### Required Operations

The core synchronization engine consumes a provider-neutral object interface:

```text
allocate_ids(count)
  -> [ObjectId]

read(id)
  -> Absent
   | Object {
       bytes,
       size,
       provider_metadata,
       server_time?
     }

create_once(id, bytes)
  -> Created {
       provider_metadata,
       server_time?
     }
   | AlreadyExists
   | AmbiguousFailure

delete(id)
  -> Deleted | Absent

list(cursor)
  -> Page {
       objects: [{ id, size, created_at?, provider_metadata }],
       next_cursor
     }
```

Contract requirements:

- `ObjectId` is opaque to core. Drive allocates IDs server-side; another
  provider may return client-generated random IDs.
- `create_once` is atomic. Two creates for the same unused ID cannot both
  succeed.
- Objects are immutable after creation. There is no generic update operation.
- `AlreadyExists` says only that the slot is occupied. Core reads and verifies
  the occupant before deciding whether its own ambiguous create committed.
- `delete` is used only by garbage collection for objects proven unreachable.
- `list` is required for recovery and garbage collection.
- Provider errors remain typed. Adapters must not convert an unsupported
  operation into a best-effort fallback.

A provider that cannot prove atomic create-once is not eligible for this
protocol.

### Optional Capabilities

```text
object_stats()
  -> Unsupported | { object_count, total_bytes }

poll_changes(cursor)
  -> Unsupported | { changed_ids, removed_ids, next_cursor }

watch_changes(cursor)
  -> Unsupported | EventStream
```

Optional capabilities reduce polling or listing. They never determine
correctness.

The provider adapter also reports authorization state:

```text
NotAuthorized
Authorizing
Authorized { expires_at?, principal { stable_id, display_label } }
AuthorizationRequired { detail }
Failed { detail }
```

Core uses this state to bind and verify the provider identity, pause and resume
the outbox, and construct the Cloud page. Transient operation failures are
separate from authorization: losing the network does not erase a valid
credential or pretend that the user must authorize again. Platform UI must not
infer provider readiness from exceptions.

### Platform Boundary

Provider request descriptions and responses are typed core contracts.

Platform-specific responsibilities are limited to:

- obtaining and storing provider credentials;
- executing typed provider network/SDK operations;
- generic local durable storage;
- connectivity and foreground/idle events; and
- rendering core-owned UI state and forwarding user actions.

Knowledge of Merkle pages, flight plans, merge policy, successor chains, or
cloud account state does not belong in TypeScript or Kotlin provider adapters.

## Immutable Merkle Store

### Object Types

Provider objects carry encrypted, authenticated envelopes. The provider can see
object IDs, sizes, and access timing but not semantic record keys.

The MVP uses:

- immutable Merkle pages containing synchronized records;
- immutable state nodes naming one Merkle root;
- a genesis state node named by the Device Setup Code; and
- optional untrusted hints or provider bookkeeping objects later.

Every envelope includes a format version, account binding, object role,
authenticated content hash, and ciphertext.

### Single Successor Chain

Each decrypted state node contains:

```text
generation
parent_node_id
parent_node_hash
merkle_root
mutation_metadata
next_slot_id
```

`next_slot_id` was allocated before the node was created and is permanently
reserved for exactly one successor.

To publish:

1. Persist the local mutation in the durable outbox.
2. Follow successors until the current tip is known.
3. Merge/rebase the local mutation against the tip's Merkle state.
4. Allocate IDs for new Merkle pages and for the next node's successor slot.
5. Create all new Merkle pages with `create_once`.
6. Create the complete successor node at the current tip's `next_slot_id`.
7. Treat that create as the linearization point.
8. Mark the outbox mutation committed only after the node is read back and
   verified or the create returns an unambiguous success.

If two clients race, exactly one occupies the successor slot. The loser reads
the winner, applies the registered record merge policy, and retries at the new
tip.

Crash behavior is bounded:

- before page creation: only the durable local outbox exists;
- after page creation but before node creation: unreachable pages are harmless
  garbage;
- during node creation: read the reserved slot to resolve ambiguity;
- after node creation: the complete state transition is committed.

There is no separate lease acquisition and no mutable-root crash window.

### Reading And Change Detection

Each client persists its last verified tip. To check for cloud changes it reads
that tip's `next_slot_id`:

- `Absent` means it is current;
- a valid node means it advances and repeats until the next slot is absent; and
- malformed, unauthenticated, or discontinuous data raises a core warning and
  is never adopted.

For the first Drive implementation, an authorized foreground client polls the
next slot every 60 seconds. Local mutations still publish immediately, and a
client checks immediately after foregrounding, reconnect, authorization, local
publication, or a provider change hint. Polling pauses while hidden, idle,
disconnected, or unauthorized.

Three-second Drive polling demonstrated the feature but is not an acceptable
product policy, especially over an airborne or metered link. Drive Changes API
support and webhook notification relays remain deferred optimizations. A future
Aerobag Cloud provider should emit SSE invalidations for the Sync Account root
and retain a one-minute-or-longer correctness poll as a backstop. Notifications
change latency, never correctness.

### Startup And History

A linked device begins at the genesis ID and follows the chain. This is
intentionally simple for the MVP but grows linearly with history.

Later work may add signed checkpoints and a best-effort mutable head hint. A
hint may accelerate startup, but correctness must remain recoverable from the
immutable chain. Chain slots are permanently consumed and cannot be deleted and
reused.

### Garbage Collection

The successor-chain spine remains reachable in the MVP. Merkle pages that lose
a publication race may be collected later.

A future sweep:

1. marks pages reachable from every retained checkpoint/head;
2. lists provider objects;
3. ignores unreachable objects newer than a grace period; and
4. deletes older unreachable pages.

Object-count equality may skip listing when the provider supplies trustworthy
statistics.

## Core Application-Facing Storage API

Application features do not read provider objects directly. They register
typed synchronized records with a core-owned store:

```text
RecordDescriptor<T> {
  logical_key
  schema_version
  encode/decode
  merge_policy
  adoption_policy
}

CloudRecordStore {
  current(record)
  replace(record, value)
  observe(record)
  pending_remote(record)
}
```

The logical key exists only inside the encrypted Merkle tree.

For each record type, core owns:

- serialization and migrations;
- local durable revision;
- cloud revision and ordering stamp;
- merge behavior;
- whether an incoming revision may be adopted immediately;
- local-history retention; and
- snapshot invalidation after adoption.

The engine emits one coherent model transition after incoming records are
verified, persisted, merged, and adopted. Platform code never receives a cloud
record and manually feeds it back into core.

## Google Drive Provider

### Storage

- Use the hidden `appDataFolder` and the narrow `drive.appdata` scope.
- Use `files.generateIds(space=appDataFolder)` for `allocate_ids`.
- Use multipart create with a supplied generated ID for `create_once`.
- Map `409 fileIdInUse` to `AlreadyExists`.
- On an ambiguous create, read the generated ID and let core verify its
  envelope.
- Use direct REST/CORS calls from web for the MVP.
- Never use Drive file update as CAS.

### OAuth Identity

The OAuth client ID is public. Trust comes from platform registration:

- production web authorizes only the exact `https://aerobag.org` origin;
- development uses a separate Google project/client authorized for localhost;
- production Android uses an Android OAuth client bound to the Aerobag package
  name and signing certificate; and
- self-hosters create their own Google project and OAuth client.

Production web and Android clients must belong to the same Aerobag Google
project so both platforms address the same per-user application data.

### Token Lifetime

The browser token model returns short-lived access tokens. Google requires a
user-driven `requestAccessToken()` call to obtain another token after expiry.
The first Drive lab observed a lifetime of approximately one hour.

It remains unresolved whether a browser can be its own long-lived token broker
using Google's lower-level public-client contract. Google's high-level Identity
Services documentation routes authorization-code exchange through a backend,
but its token endpoint currently permits browser CORS, lists `client_secret` as
optional, and documents PKCE/DPoP for public clients. This is a provider
qualification question, not an assumption on which the storage design depends.

After the MVP, resolve it with a focused browser experiment:

1. start an Authorization Code + PKCE flow with `access_type=offline`;
2. exchange the code directly from browser JavaScript;
3. verify whether the configured Web application client receives a refresh
   token;
4. persist it with a non-exportable WebCrypto DPoP key;
5. reload the page and silently obtain a new access token; and
6. record whether Google's supported production policy permits the flow.

If that qualification fails, production long-lived browser authorization needs
a small self-hostable OAuth token broker. The broker need not proxy Drive data.

The MVP does not introduce an Aerobag OAuth-token broker. Instead:

- the Cloud page shows token expiry;
- core transitions to `AuthorizationRequired` when authorization expires;
- local edits continue into the durable outbox;
- synchronization pauses without data loss; and
- `Authorize Google Drive` resumes it.

This is sufficient for the sub-hour MVP demonstration. Production background
web synchronization requires a separate decision about a token broker or
acceptable user reauthorization. Android uses its platform-appropriate OAuth
flow and credential storage.

### Provider Connection Health

Core owns the distinction between an authorization break and a temporarily
unreachable provider. Provider adapters report typed credential and operation
states; platform UI never infers severity from exception strings.

- `AuthorizationRequired`, revoked authorization, provider-identity mismatch,
  and authentication rejection are a caution. They drive the yellow `/!\`
  launcher because synchronization cannot heal until the user acts.
- network loss, timeout, provider throttling, and a provider that simply has not
  been contacted recently are informational. They appear in Cloud and Data
  Status but do not turn the launcher yellow because normal retry may heal them.
- local edits always continue into the durable outbox in either state.
- a successful provider operation clears transient status; successful
  authorization clears the authorization caution.
- status text reports the last successful connection/poll and the next required
  action without claiming that stale cloud state is current.

## Cloud Page

The Home page has a `CLOUD` destination. Core supplies two explicitly separated
regions; platform code must not classify or rearrange panels by identifier:

- `sync_account_panels` is the progressive Sync Account setup and management
  flow.
- `provider_card` is absent until a provider is selected or inherited from a
  Device Setup Code, then describes that provider's device-local authorization
  state and actions.
- `overall_status` spans both regions and states whether Cloud is operational,
  separating "a Sync Account exists" from "its provider is ready".

Web and Android render the regions side by side when space permits and stack
them on narrow displays. They only render core-projected state and perform the
semantic actions requested by core.

Within `sync_account_panels`, exactly one panel is active, working, cautionary,
or in error. The independent provider card may also offer its authorization
action. Earlier account decisions remain visible as compact completed
summaries. `Back` is a core action that unwinds one incomplete account decision:
creation returns to provider selection, while a pending account received by
Device Setup Code returns to code entry. Once an account is verified, the
`Sync Account linked`
panel always explains that the provider cannot recover the account, warns that
the Device Setup Code grants full read/write access, and tells the user to
store that code securely. `Back up Device Setup Code`, `Add another device`, and
`Unlink this device` always belong to this panel, not to provider authorization.

The initial panel contains only:

```text
Get started
[Set up from another device] [Create new Sync Account]
```

The receive path unfolds as:

1. `Get started: Set up from another device`.
2. `Scan a QR code` or paste a Device Setup Code. QR is visibly unavailable in
   the first web draft; paste is functional.
3. `Sync Account received` after core validates the code and atomically
   installs its provider and pending account descriptor.
4. The selected provider card requests authorization if it is absent.
5. `Linking account...` while core reads and verifies the genesis state.
6. `Account linked`, with `Back up Device Setup Code`, `Add another device`, and
   `Unlink this device`.

The creation path unfolds as:

1. `Get started: Create new Sync Account`.
2. Provider selection: `My Google Drive` is available; `Aerobag Cloud` is
   visible but disabled until that provider exists.
3. `Authorize Google Drive`, followed by a completed summary naming the
   authorized Google identity.
4. `Create new Sync Account on Google Drive as <identity>`.
5. `Creating Sync Account...` while core publishes genesis.
6. `Account linked`, with `Back up Device Setup Code`, `Add another device`, and
   `Unlink this device`.

The first two linked-account actions intentionally reveal the same Device Setup
Code under different task-oriented language. The backup action is suitable
for copying into a password manager; the add-device action can later add a QR
code without changing the recovery flow. `Unlink this device` unfolds a
caution panel. Core does not delete the local descriptor until the user chooses
`Yes, delete Sync Account from this device`; `Back` leaves it intact.
While any of these child panels is open, the completed linked-account panel has
no actions, so there is only one actionable account-management level.

Provider authorization is not another step in the Sync Account panel stack.
The provider card is a single stateful surface: it identifies the provider,
shows the authorized identity or current authorization problem, and offers the
appropriate authorization action. This keeps account lifecycle actions such as
recovery and unlinking separate from the device-local credential needed to use
the provider. Confirmed unlinking removes the account's provider association,
so the card disappears, but it does not revoke an otherwise-valid cached
provider credential.

The overall status uses core-owned conditions and language:

- `Cloud active: Sync Account linked, provider connected.`
- `Cloud not active: Sync Account linked, but provider requires authorization.`
- `Cloud not active: No Sync Account linked yet.`

Authorization in progress and temporary provider unavailability have distinct
truthful variants rather than claiming that the provider is connected.

The creation panel explicitly says that it always creates a new independent
Sync Account, does not find or replace another account already stored by that
provider, and directs existing-account users back to `Set up from another
device`. Account discovery is not part of the MVP.

Receiving a Device Setup Code before provider authorization is intentional. It
lets the code choose the provider and tells core which provider identity to
expect. If the subsequently authorized identity does not match, the
authorization panel reports both identities and no storage read is attempted.

The linked summary may include low-frequency operational facts such as current
generation, pending outbox work, and last successful synchronization. These are
supporting status, not a dashboard that competes with the setup flow.

The `/!\` tray also receives a core-owned cloud status record whenever the
provider is known to require user authorization. Recoverable network/provider
staleness is an informational record with `drives_caution = false`.

Controls issue semantic actions to core. OAuth itself is a platform effect
requested by core and returned as typed provider-authorization state.

The optional `Back up recovery key to Google Drive` control is deferred until
the basic device-setup flow works. When added, it requires explicit
confirmation of the changed privacy model.

## First Record: Flight Plan

The first registered record is the current flight-plan definition.

Cloud content includes the user-authored plan definition and stable row/element
identity required to reconstruct it. It excludes active leg, direct-to
execution, sequencing progress, and ownship-derived state.

Initial policy:

- whole-record last-committed replacement, using the successor-chain generation
  as the total order rather than trusting device clocks;
- retain a losing local revision in local history;
- apply an incoming plan immediately when no navigation is active;
- if navigation is active, persist the incoming revision as pending and expose
  an explicit adoption action; and
- after adoption, invalidate the core snapshot exactly once so all UI surfaces
  render the same plan.

Local flight-plan mutations use the existing session-owned mutation path. That
path emits a synchronized-record mutation; web or Android must not mirror the
flight plan back into core.

## MVP Implementation Plan

### 1. Contracts And Test Provider

- Define provider request/response, authorization-state, and capability wire
  types.
- Implement an in-memory deterministic provider with generated IDs, atomic
  create-once, injected ambiguity, failures, and delayed visibility.
- Define encrypted envelope, genesis, state-node, Merkle-page,
  Device-Setup-Code, and local-engine-state contracts.
- Select and version cryptographic primitives and derivation labels.
- Add model tests for races, crashes, retries, corruption, and chain traversal.

### 2. Core Store And Synchronization Engine

- Implement generic local persistence and the durable outbox.
- Implement Merkle record read/replace and typed record registration.
- Implement genesis creation and Device Setup Code import/export.
- Implement successor publication, conflict retry, polling, and remote
  adoption.
- Expose passive Cloud page and Data Status models.
- Keep provider operations asynchronous and outside platform UI threads.

### 3. Web Google Drive Adapter

- Promote the experiment's OAuth and REST plumbing into a production adapter;
  do not retain lab types or duplicate storage policy.
- Implement ID allocation, read, create-once, delete, and list.
- Persist only non-token provider configuration in ordinary web storage.
- Keep access tokens in memory and surface expiry/reauthorization.
- Use a production client ID configured by deployment and a separate dev
  client ID for localhost.

### 4. Cloud Page

- Add the Home-page destination and provider selection.
- Add the progressive setup flow, Google authorization, Sync Account creation,
  Device Setup Code copy/paste, unlink, and sync controls.
- Render only core-computed enablement, status, and errors.
- Add a small debug diagnostic showing provider request, node generation, and
  adoption source for acceptance testing.

### 5. Flight-Plan Integration

- Register the flight-plan definition descriptor.
- Emit record mutations from the single core flight-plan mutation path.
- Reconstruct and adopt incoming plans through core.
- Implement the active-navigation adoption guard.
- Verify that FP page, map route, CDI inputs, and data grid all observe the same
  adopted session snapshot.

### 6. End-To-End Crossfill

- Use two independent browser profiles or one browser plus Android. Two tabs
  sharing local storage do not count.
- Authenticate both clients to the same Google account.
- Create the cloud account on client A.
- Authenticate client B independently and set it up with A's Device Setup Code.
- Edit the inactive flight plan on A and observe it on B within one Drive poll.
- Edit it on B and observe the reverse update on A.
- Verify diagnostics show a Drive successor node and `source = cloud`, not
  local-storage or tab-broadcast propagation.
- Let one browser token expire or simulate expiry, edit locally, reauthorize,
  and verify the outbox publishes without losing the edit.
- Verify expired/revoked authorization creates a yellow `/!\` caution, while an
  injected network timeout creates only an informational status record.

The first successful manual demonstration should be captured as a screen
recording. The same flow runs automatically against the deterministic provider
in CI; real Google authorization remains a manual/provider qualification test.

## Required Tests

- Exactly one concurrent create occupies a successor slot.
- A `409` loser reads, merges, and retries at the next generation.
- An ambiguous create resolves by reading and authenticating the reserved ID.
- Crashes before pages, after pages, and during node creation preserve the
  prior committed state and durable outbox.
- Missing, corrupt, discontinuous, or unauthenticated nodes are rejected.
- A reader missing many polls follows every successor in order.
- Polling resumes after foreground, reconnect, and reauthorization.
- Token expiry pauses cloud effects but not local flight-plan editing.
- Authorization failure drives caution; transient network failure does not.
- Device setup reconstructs account access without transferring Google
  credentials.
- Two devices editing sequentially crossfill the flight plan in both
  directions.
- Concurrent flight-plan replacement resolves deterministically and retains
  the losing local revision.
- A remote plan arriving during active navigation is persisted but not adopted.
- Snapshot invalidation after remote adoption occurs exactly once.
- Provider/platform code contains no flight-plan, Merkle, or merge policy.
- Logs, provider objects, and report endpoints contain no OAuth token, Device
  Setup Code, root secret, or plaintext flight plan.

## After The MVP

- Android Google Drive adapter and native OAuth lifecycle.
- Recovery-key backup in Drive.
- Preferences, aircraft, package policy, saved plans, and traces.
- Signed checkpoints, bounded startup traversal, and chain compaction.
- Merkle-page garbage collection and provider accounting.
- Drive Changes polling and optional webhook notification relay.
- Production web long-lived authorization strategy.
- Aerobag UCS implemented against the same create-once provider interface.
- WebDAV qualification using `If-None-Match: *` or another proven atomic
  create-once primitive.
- Provider migration and multiple package-policy profiles.
- QR Device Setup Codes and camera scanning.

## Future State Placement

Likely cloud-synchronized records:

- current and saved flight-plan definitions;
- user preferences intended to follow the user, including data-grid choices
  and ownship-symbol preference;
- aircraft definitions and performance configuration;
- desired offline-package configuration, initially as one shared profile; and
- captured GPS/ownship traces when explicitly enabled.

Device-local state:

- active leg, sequencing, direct-to execution, and other navigation execution;
- ownship state and sensor history;
- viewport, open trays, selected page, and transient UI state;
- installed package inventory and bytes, progress, storage budget, and GC;
- network constraints and provider credentials;
- package, live-feed, and development-server endpoints;
- physical-screen dimming and similar device display controls; and
- debug logs and diagnostic captures unless explicitly exported.

## Future Aerobag UCS Constraints

An Aerobag-hosted provider should implement the same allocate/read/create-once/
delete/list interface. Native mutable CAS or event streams are optional
optimizations and must not fork the core storage model.

Previously agreed service constraints remain:

- cloud account creation is explicit, not automatic at first app use;
- anonymous accounts receive a small bounded allowance;
- verified identities may receive larger quota entitlements without replacing
  the opaque cryptographic account identity;
- raw client IP addresses are discarded immediately after producing a
  server-keyed pseudonymous correlation value;
- creation throttles must account for carrier NAT, airline, airport, and shared
  household networks, so early limits should slow or challenge creation rather
  than breaking existing accounts;
- every tier has hard ceilings for storage, operations, bandwidth, connections,
  and egress;
- operators can make an account read-only, suspend it, or delete it during
  abuse response; and
- operators can inspect usage and identity/entitlement metadata but cannot
  decrypt synchronized payloads.

## Deferred Decisions

- Exact AEAD, signature, KDF, and envelope formats pending cryptographic review.
- Polling/backoff policy after Drive change hints or Aerobag Cloud SSE exist.
- Checkpoint/compaction strategy for the immutable chain.
- Production web refresh-token architecture.
- Recovery-key rotation and re-encryption UX.
- Cloud quotas and whether trace synchronization is enabled by default.
