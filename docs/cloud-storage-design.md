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
  pairing tokens.
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
- QR pairing; copy/paste pairing tokens are sufficient for the first slice.
- Google webhook relays or background synchronization after the web access
  token expires.
- Garbage-collecting the immutable successor-chain spine.
- Synchronizing traces, aircraft, preferences, or offline-package policy before
  the flight-plan slice works end to end.
- Aerobag UCS, WebDAV, or provider migration.
- Cloud storage as a package or live-feed cache.

## Identity, Keys, And Pairing

### Local-First Identity

A fresh installation generates a device ID and stores all state locally. It
does not allocate cloud objects or contact a cloud provider.

The Cloud page offers:

- `Create cloud account`, which publishes the current eligible local state;
- `Link existing account`, which imports a pairing token;
- `Pair another device`, which displays a copyable pairing token;
- `Disconnect`, which stops cloud access without deleting local state; and
- a later, separately confirmed `Delete cloud account` operation.

Linking never transfers provider credentials. Each device independently
authenticates to the provider named in the token.

### Root Secret

Each Aerobag cloud account has a random 256-bit root secret. Core derives
independent material with a versioned KDF and distinct context labels:

- payload encryption keys;
- node and record authentication/signing keys;
- account locator material; and
- recovery/key-wrapping material.

Cryptography and serialization are implemented once in core. Web and Android
must not implement parallel cryptographic formats.

The pairing token contains the root secret, provider kind, provider-specific
non-secret configuration, genesis object ID, and contract versions. Possession
of the token grants complete account access. It must not appear in a normal URL
query or path.

### Optional Provider Recovery

By default, the root secret exists only on paired devices. Losing all paired
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

The provider adapter also reports credential state:

```text
Disconnected
Connecting
Ready { expires_at? }
NeedsUserAction
Failed { category, detail }
```

Core uses this state to pause and resume the outbox and to construct the Cloud
page. Platform UI must not infer provider readiness from exceptions.

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
- a genesis state node named by the pairing token; and
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

For the MVP, foreground clients poll the next slot frequently enough to target
crossfill within five seconds. Polling backs off while hidden, idle,
disconnected, or unauthorized and runs immediately after foregrounding,
reconnect, local publication, or a provider change hint.

Drive Changes API support and webhook notification relays are deferred
optimizations.

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
- core transitions to `NeedsUserAction` when authorization expires;
- local edits continue into the durable outbox;
- synchronization pauses without data loss; and
- `Reconnect Google Drive` resumes it.

This is sufficient for the sub-hour MVP demonstration. Production background
web synchronization requires a separate decision about a token broker or
acceptable user reauthorization. Android uses its platform-appropriate OAuth
flow and credential storage.

### Provider Connection Health

Core owns the distinction between an authorization break and a temporarily
unreachable provider. Provider adapters report typed credential and operation
states; platform UI never infers severity from exception strings.

- `NeedsUserAction`, revoked authorization, and authentication rejection are a
  caution. They drive the yellow `/!\` launcher because synchronization cannot
  heal until the user acts.
- network loss, timeout, provider throttling, and a provider that simply has not
  been contacted recently are informational. They appear in Cloud and Data
  Status but do not turn the launcher yellow because normal retry may heal them.
- local edits always continue into the durable outbox in either state.
- a successful provider operation clears transient status; successful
  authorization clears the authorization caution.
- status text reports the last successful connection/poll and the next required
  action without claiming that stale cloud state is current.

## Cloud Page

The Home page gains a `CLOUD` destination. Core supplies the complete page
model and action enablement.

Minimum controls:

- provider scheme selector, with only `Google Drive` enabled initially;
- `Connect Google Drive` / `Reconnect Google Drive`;
- `Disconnect Google Drive`;
- `Create cloud account`;
- `Link existing account` with a pairing-token text input;
- `Pair another device`, exposing a copyable token;
- `Sync now`; and
- account and synchronization status.

Minimum status:

- provider and credential state, including expiry;
- linked/unlinked account state;
- local outbox count;
- current verified cloud generation;
- last successful poll and publication;
- pending remote records;
- paused/offline/auth-required state; and
- actionable error text.

The `/!\` tray also receives a core-owned cloud status record whenever the
provider is known to require user authorization. Recoverable network/provider
staleness is an informational record with `drives_caution = false`.

Controls issue semantic actions to core. OAuth itself is a platform effect
requested by core and returned as typed credential state.

The optional `Back up recovery key to Google Drive` control is deferred until
the basic pairing flow works. When added, it requires explicit confirmation of
the changed privacy model.

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

- Define provider request/response, credential-state, and capability wire
  types.
- Implement an in-memory deterministic provider with generated IDs, atomic
  create-once, injected ambiguity, failures, and delayed visibility.
- Define encrypted envelope, genesis, state-node, Merkle-page, pairing-token,
  and local-engine-state contracts.
- Select and version cryptographic primitives and derivation labels.
- Add model tests for races, crashes, retries, corruption, and chain traversal.

### 2. Core Store And Synchronization Engine

- Implement generic local persistence and the durable outbox.
- Implement Merkle record read/replace and typed record registration.
- Implement genesis creation and pairing-token import/export.
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
- Add Google authorization, account creation, pairing-token copy/paste,
  disconnect, and sync controls.
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
- Authenticate client B independently and link it with A's pairing token.
- Edit the inactive flight plan on A and observe it on B within five seconds.
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
- Pairing reconstructs account access without transferring Google credentials.
- Two devices editing sequentially crossfill the flight plan in both
  directions.
- Concurrent flight-plan replacement resolves deterministically and retains
  the losing local revision.
- A remote plan arriving during active navigation is persisted but not adopted.
- Snapshot invalidation after remote adoption occurs exactly once.
- Provider/platform code contains no flight-plan, Merkle, or merge policy.
- Logs, provider objects, and report endpoints contain no OAuth token, pairing
  token, root secret, or plaintext flight plan.

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
- QR pairing and camera scanning.

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
- Exact foreground polling and idle-backoff intervals.
- Checkpoint/compaction strategy for the immutable chain.
- Production web refresh-token architecture.
- Recovery-key rotation and re-encryption UX.
- Cloud quotas and whether trace synchronization is enabled by default.
