# User Cloud Storage Design

## Status

Aerobag Cloud Service (ACS) is the sole cloud-storage provider. Aerobag remains
fully local unless the user explicitly creates or links a Sync Account.

Core owns the synchronized model, encryption, merge policy, persistence,
synchronization state machine, retries, and Cloud-page UI model. Platform code
only executes bounded HTTP and SSE effects planned by core, persists opaque
core state, and renders core-provided controls.

## Design Decisions

- The synchronized application state is an encrypted Merkle tree.
- ACS stores opaque immutable objects and one distinguished mutable account
  root. Compare-and-swap of that root is the publication linearization point.
- A losing writer reads the winning root, applies the common record merge
  policy, builds a new root, and retries.
- Normal synchronization follows known object IDs. Listing is reserved for
  recovery, accounting, and garbage collection.
- SSE notifications are latency hints. Polling and root traversal remain
  sufficient for correctness.
- Active navigation state is device-local. A synchronized flight-plan
  definition cannot silently replace a plan currently being flown.
- Remote changes pass through the same core-owned model and invalidation path
  as local changes.

## Identity And Device Setup

A fresh installation generates a device ID and stores all state locally. Each
Sync Account has a random 256-bit root secret. Core derives independent account
locator, payload-encryption, and request-signing material using the versioned
cloud KDF.

The Device Setup Code contains the root secret, ACS base URL, and derived
account locator. It never places secrets in URLs, logs, or ordinary platform
configuration. Accepting a setup code atomically installs the pending account
descriptor; core then reads and verifies the account root before declaring the
device linked.

The user-visible lifecycle is:

1. Create a new Sync Account, or paste/scan a Device Setup Code from an
   existing linked device.
2. Core creates or verifies the ACS account and genesis state.
3. The Cloud page reports the linked account, synchronization status, and any
   actionable failure.
4. The user may expose the same setup code for backup or another device, or
   explicitly unlink this device.

Unlinking removes the local account descriptor. It does not delete cloud data
or alter another linked device.

## ACS Boundary

Core constructs complete provider requests, including URLs, headers, request
bodies, signatures, response limits, and expected operation state. Web and
Android execute those opaque requests and return status plus bounded response
bytes. Platform code must not interpret Merkle objects, choose merge behavior,
or manufacture cloud operations.

ACS exposes account creation, immutable object storage, root read/CAS, bounded
object listing, and account-scoped SSE tickets. Request authentication is
derived from the Sync Account secret and signed by core. See
`docs/refactor/aerobag-cloud-server.md` and
`crates/product-contracts/src/aerobag_cloud.rs` for the wire contract.

## Synchronized Records

Core registers each synchronized record type with:

- serialization and migration behavior;
- durable local and cloud revisions;
- merge and ordering policy;
- adoption rules while navigation is active; and
- snapshot invalidation after adoption.

The engine emits one coherent model transition only after incoming records are
verified, persisted, merged, and adopted. Platform code never receives an
application record and manually feeds it back into core.

Current synchronized state includes the flight-plan definition, offline
package preferences, debug settings, and aircraft-library selections/private
definitions. Flight-plan execution state, ownship, viewport, open trays, and
other transient navigation state remain device-local.

## Flight Plans

Cloud flight-plan content includes the user-authored definition and stable row
identity needed to reconstruct it. It excludes active leg, direct-to execution,
sequencing progress, and ownship-derived state.

Whole-record replacement is ordered by a semantic mutation stamp created by
core. The durable outbox preserves the stamp across retries. If remote plan
state arrives while local navigation is active, core keeps the active local
execution state and surfaces the pending definition for deliberate adoption.

## Offline Operation And Health

Local edits commit immediately and enter a durable outbox. Network failures do
not discard edits. Core retries with bounded backoff and reports provider
connection state through the Cloud page and unified data-quality system.

Transient unavailability is informational while durable local work remains
safe. Authentication, integrity, account mismatch, or repeated publication
failure is a caution requiring user attention. Platform UI does not infer
severity from HTTP or exception strings.

## Required Tests

- Account creation, setup-code linking, unlinking, and relinking.
- Root-CAS races where both writers eventually converge.
- Ambiguous writes resolved by reading the distinguished root.
- Offline edits surviving restart and publishing after reconnection.
- Corrupt, oversized, or unauthenticated responses rejected before adoption.
- Remote adoption invalidating every derived flight-plan/rendering projection.
- Web and Android executing the same core-planned HTTP contract without
  storage or application policy in platform code.
- Crossfill of every registered synchronized record type.

## Deferred Work

- User-facing conflict history and recovery tools.
- Large-object chunking beyond current account limits.
- Multi-user collaboration and shared ownership.
- Server-side horizontal scaling beyond the measured need.
