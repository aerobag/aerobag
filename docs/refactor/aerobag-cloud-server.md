# Aerobag Cloud Server Plan

## Goal

Implement Aerobag Cloud Server (ACS) as a low-latency, self-hostable storage
provider for the existing cloud synchronization engine. ACS uses the same
encrypted Merkle representation and record synchronization engine as Google
Drive, but provides a native distinguished-root CAS instead of inheriting
Drive's immutable-successor compromise. It also adds SSE change notification,
bounded anonymous service, production operations, and pipeline-health
visibility.

The first acceptance test is two independently stored clients linked to one
Sync Account. A flight-plan or offline-package preference edit on either client
must appear promptly on the other without waiting for a periodic poll.

## Architectural Boundary

ACS implements the existing provider operations:

- `allocate_ids`
- `read`
- atomic `create_once`
- `delete`
- `list`
- optional `object_stats`
- `read_root`
- atomic `compare_and_swap_root`
- `watch_root`

Core continues to own encryption, immutable Merkle pages, publication races,
record merge/adoption policy, retry policy, and Cloud UI state. A narrow
core-owned root-publication abstraction accounts for a real provider capability
difference: ACS uses native CAS on a distinguished root, while Drive uses its
existing create-once successor slots because Drive cannot safely CAS. Merkle
construction and everything above final root publication remain single-path.
Web and Android execute typed HTTP and streaming effects without understanding
those storage semantics.

The server sees an opaque account locator, opaque object IDs, object sizes, and
access timing. It does not receive the root secret, plaintext record keys,
plaintext synchronized state, or payload encryption keys.

### Distinguished Account Root

The word `root` is overloaded and must be qualified:

- The **root secret** never leaves linked clients.
- Every ACS account has one **distinguished account root** at a fixed address
  derived by convention from the account locator.
- The account-root value contains a server revision, an authenticated value
  hash, encrypted root metadata, and visible references to the immutable
  Merkle pages at the top of the current tree.
- The account root is the only mutable storage location. Merkle pages remain
  immutable and retain structural sharing across revisions.
- ACS knows the root address, revision, visible object graph, and update timing.
  It does not know plaintext record keys or encrypted root/page contents.

There is no additional immutable `root_object_id`. The fixed account root is
the CAS record. This avoids a needless pointer level and gives ACS one stable
place to observe, compare, and atomically advance committed application state.

## Service Storage

Use one standalone Rust daemon, tentatively `aerobag-cloud-serverd`, backed by
SQLite WAL for the MVP. It is a service, not preprocessor or live-feed logic.

The database stores:

- opaque accounts, authentication public keys, status, quota class, and usage;
- one distinguished account-root revision and value per account;
- immutable objects keyed by account locator and object ID, including visible
  child-object references authenticated as object metadata;
- a monotonically increasing event sequence for each account;
- bounded retained event history for SSE reconnection; and
- pseudonymous abuse counters and operational statistics.

### Inline And Filesystem Payloads

SQLite is the metadata database and also stores small ciphertext payloads
inline. An implementation-defined threshold, initially around `64-256 KiB`,
selects storage placement without changing the client-visible object contract:

- small Merkle pages, flight-plan state, and preferences remain inline in the
  object row;
- larger immutable payloads live in a filesystem blob store; and
- the object row contains exactly one of `inline_ciphertext` or
  `blob_storage_key`, plus size and content hash.

The threshold is a server efficiency choice, not an account quota or protocol
limit. Clients neither select nor observe the placement.

Inline `create_once` is one SQLite transaction. A large create reserves its
object ID and account quota transactionally, streams into a temporary file,
checks size and hash, durably installs the immutable blob, and only then marks
the object readable. Startup reconciliation finishes or removes interrupted
reservations. Files left without committed metadata are harmless GC candidates;
a committed readable row must never name missing bytes.

The blob-store interface begins with a local filesystem implementation for
dev-stack, production MVP, and ordinary self-hosting. An S3-compatible backend
can replace it later without changing the provider protocol. Backups and
restore tests must cover SQLite and referenced filesystem blobs as one logical
store.

Merkle pages are staged with `create_once`. A duplicate object ID returns
`AlreadyExists`; it never overwrites bytes. Staging does not emit an SSE event.

Publication completes with:

```text
compare_and_swap_root(
    expected_revision,
    expected_root_hash,
    new_encrypted_root,
    new_child_references
)
```

ACS verifies that every directly referenced page exists, atomically compares
and replaces the distinguished root, and appends exactly one event in the same
SQLite transaction. The root CAS is the publication linearization point. A
losing writer reads the winning root, applies the common record merge policy,
builds a new root, and retries. An ambiguous result is resolved by reading the
fixed root.

The service starts as one process with one database. Multi-process writers,
external pub/sub, and horizontal scaling are deferred until measured load
requires them.

## Account Authentication

Derive request-signing material independently from encryption and account
locator material using the versioned core KDF. Account creation registers the
derived signing public key. Subsequent requests are signed over method, path,
body hash, timestamp, and nonce. The server enforces a bounded clock window and
nonce replay protection.

The provider-neutral Device Setup Code contains one tagged provider
configuration. An ACS configuration includes its non-secret provider URL, so a
paired device selects the same self-hosted or Aerobag-operated ACS
automatically. A Google Drive configuration instead contains the Drive account
binding and genesis object ID. The durable Sync Account secret must never
appear in a URL, server log, or ordinary platform configuration.

### Phase-1 Contract Draft

The reviewable `ACS1` Rust wire types live in
`crates/product-contracts/src/aerobag_cloud.rs`. Phase 1 defines the following
HTTP resources but deliberately does not expose them through a network daemon:

| Method | Resource | Purpose |
| --- | --- | --- |
| `POST` | `/cloud/v1/account-challenges` | Obtain a short-lived, rate-limited account-creation challenge. |
| `POST` | `/cloud/v1/accounts` | Create an account and register its signing public key. |
| `GET`, `PUT` | `/cloud/v1/accounts/{account}/objects/{object}` | Read or create once one immutable object; server-owned GC performs deletion. |
| `GET` | `/cloud/v1/accounts/{account}/objects` | Bounded cursor-based object listing for recovery and GC diagnostics. |
| `GET`, `PUT` | `/cloud/v1/accounts/{account}/root` | Read or compare-and-swap the fixed account root. |
| `POST` | `/cloud/v1/accounts/{account}/event-tickets` | Mint a short-lived SSE bearer ticket using a signed request. |
| `GET` | `/cloud/v1/events?ticket=...` | Open the account-scoped SSE stream. |
| `GET` | `/cloud/v1/status` | Read bounded operator statistics. |

Except for unsigned challenge issuance, the ticket-bearing `EventSource`
request, and operator status, requests carry `Aerobag-Contract`,
`Aerobag-Account`, `Aerobag-Key-Id`,
`Aerobag-Signature-Algorithm`, `Aerobag-Timestamp-Ms`, `Aerobag-Nonce`,
`Aerobag-Body-SHA256`, and `Aerobag-Signature` headers. `Ed25519` is the only
`ACS1` signature algorithm. The signature input is exactly:

```text
ACS1
METHOD
CANONICAL_REQUEST_TARGET
ACCOUNT_LOCATOR
SIGNING_KEY_ID
TIMESTAMP_EPOCH_MS
NONCE_BASE64URL
LOWERCASE_BODY_SHA256
```

The canonical request target is the percent-encoded path plus query parameters
sorted by encoded key and value; it never includes scheme, authority, or a
fragment. The signed byte string includes a final newline. An empty request
body uses the SHA-256 of zero bytes. Account creation
is self-signed by the public key in its body. Other requests are verified
against the key registered for the opaque account locator.

The `ACS1` KDF is HKDF-SHA256 with salt `aerobag-cloud-account-v1` and separate
labels `account-locator`, `payload-encryption`, and
`request-signing-ed25519-seed`. It emits a 256-bit account locator, a
ChaCha20-Poly1305 payload key, and a 256-bit Ed25519 seed. The key ID is the
first 128 bits of SHA-256 over the Ed25519 public key; request nonces are 128
random bits. Binary values use unpadded base64url in JSON and headers. The
account locator and key ID are derived and checked again when importing an ACS
provider payload from the generic `AB3` Device Setup Code. The ASCII
representation remains `AB3.<base64url payload and binary checksum>` so the
decoder version is visible before decoding. Its logical payload contains a
256-bit root secret and exactly one tagged provider configuration:

- Google Drive: stable principal fingerprint, display hint, and genesis object
  ID.
- Aerobag Cloud: provider base URL and derived 256-bit account locator.

`AB3` is an application/core contract, not an ACS server contract; the blind
server never reads it. AB3 replaces AB2 without a compatibility decoder or
automatic migration. Google Drive continues using its immutable-successor
publication protocol independently of the setup-code encoding.

#### Device Setup Code Codec Experiment

An isolated production WASM experiment compared generic JSON, postcard, and
Protocol Buffers payloads. The representative Google payload used a 256-bit
secret and principal fingerprint, a real-size Drive object ID, and a 20-byte
display hint. The ACS payload used a 256-bit secret and locator plus
`https://aerobag.org/cloud/`. Code lengths include the visible `AB3.` prefix,
unpadded base64url, and an eight-byte binary checksum.

| Codec | Google payload/code | ACS payload/code |
| --- | ---: | ---: |
| JSON with base64url binary fields | 298 B / 412 chars | 221 B / 310 chars |
| postcard | 138 B / 199 chars | 92 B / 138 chars |
| Protocol Buffers via `prost` | 145 B / 208 chars | 98 B / 146 chars |

The existing Google Drive `AB2` JSON shape is about 338 payload bytes and 472
characters for the same representative values.

Each codec's encode and decode paths were retained in the complete
`app-wasm`, built with the `wasm-perf` profile and Binaryen 129 `wasm-opt -O2`.
Relative to an otherwise identical control export:

| Codec | Raw WASM growth | gzip -9 growth | Brotli q11 growth |
| --- | ---: | ---: | ---: |
| JSON | 19,923 B | 6,013 B | 2,970 B |
| postcard | 6,181 B | 1,941 B | 2,065 B |
| Protocol Buffers / `prost` | 17,485 B | 6,575 B | 5,834 B |

The base module was 5,139,880 raw bytes and 1,646,077 gzip bytes in the control
build. These are marginal parallel-codec measurements, so replacing the current
AB2 implementation may be smaller; they intentionally price a reachable
encoder and decoder rather than merely adding an unused dependency.

After replacing AB2 in the actual product, optimized WASM grew by 13,919 raw
bytes, 5,545 gzip bytes, and 4,402 Brotli bytes. The resulting module is
1,651,550 bytes with gzip `-9`.

Protocol Buffers costs only 8-9 Device Setup Code characters over postcard and
about 4.6 KiB more gzip-compressed WASM. In return, numbered optional fields and
reserved tags provide a substantially cleaner compatibility discipline than a
positional postcard schema. AB3 therefore uses Protocol Buffers via `prost`.
Because `prost` discards unknown fields while decoding, core retains the
original imported AB3 bytes for re-sharing rather than decoding and
re-encoding them through an older schema. The checked-in `.proto` schema never
reuses field numbers, reserves removed fields, and rolls to `AB4` only for
incompatible semantic changes.

Signed requests are accepted within a five-minute server-clock window. A
nonce is single-use for that account and signing key throughout that window;
a retry after an ambiguous transport failure uses a new nonce and resolves the
operation by reading create-once content or the fixed root. SSE tickets expire
after two minutes and are account- and endpoint-scoped. The signed ticket
request includes the client's last accepted event sequence. That cursor is
bound into the ticket so a browser can resume after minting a new ticket even
though a newly constructed `EventSource` cannot set `Last-Event-ID`. Native
reconnects on the same stream also use ordinary `Last-Event-ID`. These values
are part of the contract-review gate, not yet a deployed promise.

Encrypted values carry sorted, unique visible child IDs. Contract code defines
canonical AEAD associated data containing `ACS1`, value kind, object/root ID,
and those child IDs. The authenticated value hash additionally binds the
ciphertext SHA-256. The server therefore sees tree edges for GC but cannot
silently move ciphertext or alter those edges without client verification
failing.

Create-once returns `created` or idempotent `already_exists`; the same object ID
with different authenticated data is HTTP 409. Root CAS returns `committed` or
HTTP 409 with the current revision and hash. Other stable error mappings are:
400 malformed, 401 missing/expired/replayed authentication, 403 quota or
suspension, 404 missing, 409 state conflict or missing child, 413 bounded body
or response exceeded, 429 rate limit, 503 read-only, and 500 internal failure.
Error bodies always use the typed `AcsErrorResponse`; only retryable errors set
`retry_after_ms`.

A new account has no root: root read returns 404, and its first CAS expects
revision `0` and hash `null`. The first successful CAS creates revision `1`.
Every later successful CAS increments revision and SSE sequence exactly once in
the same transaction; object staging never changes either counter.

## SSE Change Notification

Each account has an SSE event sequence associated with distinguished-root
changes.

- Connection begins with
  `ready { sequence, root_revision, root_hash }`.
- A successful root CAS emits
  `root-changed { sequence, root_revision, root_hash }` after commit.
- Heartbeats include the latest sequence, root revision, and root hash.
- Clients reconnect with `Last-Event-ID` and receive retained events after that
  cursor.
- If the cursor is older than retained history, ACS emits `reset`; core performs
  an immediate distinguished-root read.

The event does not include `root_object_id`: no such varying ID exists. The
root address is fixed by convention. A client that observes a new revision
reads and cryptographically verifies that fixed root. SSE remains a change hint
rather than an alternative state-transfer path.

The browser cannot attach arbitrary authorization headers to native
`EventSource`. Core therefore obtains a short-lived SSE ticket through a signed
request. The ticket is narrowly scoped to one account and expires quickly; it
is not the root secret or a durable bearer capability. Web and Android use the
same ticket protocol.

Core owns event interpretation, root comparison, reconnect/backoff policy, and
sync invalidation. Platform code only opens the stream and forwards typed
events or transport failure.

### Heartbeat And Poll Policy

Use the same shared SSE transport policy as live-feeds rather than defining
ACS-specific liveness timers:

| Policy | Value |
| --- | --- |
| Server heartbeat interval | `30 seconds` |
| Client idle timeout | `65 seconds` |
| Connection timeout | `5 seconds` |
| Reconnect delays | `5, 10, 20, 40, 65, 65... seconds` |

- Move these values into one Rust contract consumed by the live-feed daemon,
  ACS daemon, and app core. Core supplies transport timeout policy to platform
  adapters; Kotlin and TypeScript must not carry independent numeric copies.
- Read the distinguished root immediately at startup, foregrounding,
  authorization, provider reconnection, connectivity recovery, and after a
  local publication.
- While the stream is healthy and its event sequence agrees with the client's
  cursor, do not perform ordinary one-minute polling.
- Perform a low-frequency correctness audit, initially every 30 minutes, by
  reading the distinguished root. This guards against implementation defects;
  notifications are still not correctness-critical.
- Reconnect with exponential backoff capped at 65 seconds. Reconnection
  always performs an immediate root check.
- Pause streaming and auditing under the same hidden/idle policy as the rest of
  cloud synchronization.

### Garbage Collection

Immutable objects expose only their child object IDs as visible, authenticated
metadata. Record keys and values remain encrypted. This reveals tree shape and
update patterns, which is an accepted ACS tradeoff for autonomous GC.

ACS marks from the current distinguished root and sweeps unreachable objects
older than a grace period, initially 24 hours. The grace period protects a
client finishing a read from a recently superseded root and collects:

- pages staged by a writer that crashed before root CAS;
- pages staged by a writer that lost a CAS race and never reused them; and
- pages reachable only from superseded roots.

Shared pages remain reachable from the current root. A sweep records the root
revision it marked and rechecks that revision before deletion so a concurrent
root advance cannot cause reachable data to be collected. Recent-root retention
may be added for operational rollback, but an unbounded successor-chain spine
is not part of the ACS design.

## Client Product Flow

Enable the existing `Aerobag Cloud` provider option in the core-owned Cloud
state machine.

- Provider selection displays the configured ACS URL.
- Account creation is explicit and creates a fresh opaque account.
- ACS requires no OAuth panel; account capability derives from the Sync Account
  secret.
- Device Setup Code and QR setup reuse the existing pairing flow.
- Provider status distinguishes connected, reconnecting, unavailable,
  read-only, suspended, and account-invalid states.
- A temporary network outage remains informational. Authorization/account
  failure requiring user action drives the `/!\` caution.

The ACS provider adapter belongs in core. It maps generic provider operations
to typed ACS HTTP requests and parses responses once for both platforms.

## Abuse Protection Required For MVP

The protocol and source are public. Official-client checks, hidden endpoints,
or embedded shared credentials provide no meaningful protection.

### Per-Account Limits

- Start anonymous accounts with a `1 MiB` aggregate stored-byte allowance. No
  anonymous object can exceed the account's total allowance. This is an abuse
  policy, not a storage-format constant.
- Do not select a large object-count allowance by guess. Measure the Merkle
  layout and set a separate conservative metadata/object-count ceiling before
  deployment so tiny-object abuse cannot consume unbounded database rows.
- Verified or operator-blessed accounts receive explicitly larger per-object,
  aggregate-storage, operation, and egress limits. Large GPS-track upload is
  unavailable to anonymous accounts.
- Bound operations, ingress, egress, list page size, and concurrent SSE streams.
- Enforce storage and object-count quotas transactionally with object creation.
  Stored-byte accounting includes ciphertext, attacker-controlled object IDs,
  visible graph edges, and a conservative fixed SQLite-row allowance; otherwise
  dense tiny-object graphs can amplify a nominally small quota into a large
  metadata database.
- Reject malformed or oversized IDs, headers, bodies, cursors, and error text.

Current-format sizing supports the smaller allowance. A synthetic state with a
20-waypoint flight plan and 29 package selections produced an `11,768`-byte
encrypted page plus a `573`-byte state node; two generations occupied `13,935`
bytes total. Before freezing quotas, add a churn test that publishes rapid
legitimate edits and measures the temporary unreachable bytes retained until
GC. If `1 MiB` proves tight, fix publication coalescing or GC latency before
casually multiplying anonymous storage.

### Creation And Network Limits

- Immediately transform source IPs into server-keyed HMAC pseudonyms; never
  persist or log raw addresses. IPv4 identities use the full address; IPv6
  identities use the /64 prefix so privacy-address rotation does not mint a new
  limiter identity on every request.
- Apply a small time-windowed anonymous account-creation allowance per
  pseudonym. Shared-NAT collateral damage is unavoidable, so creation pressure
  must not impair already-created accounts.
- Escalate creation pressure from ordinary allowance to a server challenge or
  proof-of-work, then rejection. Proof-of-work raises automated Sybil cost but
  is not treated as a security boundary.
- Apply token-bucket operation and bandwidth limits per account and per network
  pseudonym.
- Cap concurrent SSE streams per account, per pseudonym, and globally.

### Global Safety And Operations

- Enforce hard global ceilings for disk use, write rate, egress, and open
  connections.
- Enter global read-only mode before disk exhaustion.
- Provide operator controls to adjust quota or make an opaque account
  read-only, suspended, or deleted.
- Never log Device Setup Codes, signing material, ciphertext bodies, access
  tokens, SSE tickets, or raw IP addresses.
- Back up the SQLite database and test restoration before calling the service
  production-ready.

The server cannot determine whether encrypted bytes are legitimate Aerobag
state. The MVP defense against blind dead-drop use is therefore finite quota,
costly account creation, bounded egress, anomaly visibility, and immediate
operator containment.

### Future Large Application Objects

Core does not split a large logical object merely to satisfy the anonymous
quota policy. A GPS track may be one immutable object stored externally by
ACS, while a small encrypted record in the Merkle tree names that object and
contains its application metadata.

Chunking is justified only by a product requirement such as partial time-range
access, incremental publication while recording, provider-specific size
limits, or measured retry cost on unreliable links. If the only problem is
retrying a 25 MiB upload, a generic resumable `create_once` transport is
preferable to making chunk boundaries part of the application data model.

Verified identities, paid entitlements, adaptive reputation, and large trace
storage are deferred. They may grant larger quota without replacing the opaque
cryptographic Sync Account identity.

## Deployment

### Development Stack

`tools/run_dev_stack.py` will:

- build and launch `aerobag-cloud-serverd`;
- listen internally on the currently unused `127.0.0.1:18096` by default;
- persist its database under the dev-stack data root, outside published
  artifacts;
- expose `/cloud/` through the `:18080` front door;
- advertise that URL in development client configuration; and
- include the daemon in dev-stack status and lifecycle handling.

The daemon's HMAC secret is runtime infrastructure state, not application data.
Development consumes it directly from the existing operator-owned credentials
tree at:

```text
/root/aerobag-credentials/dev-stack/aerobag-cloud-server.bin
```

It must never be generated under or copied through an artifact publication
root.

### Production

`tools/deploy_prod.py` will:

- build and install the release daemon;
- install and maintain `aerobag-cloud-server.service`;
- persist state under `/mnt/aerobag-data/cloud-storage`, never under the artifact tree;
- install server-only secrets under `/etc/aerobag/secrets`;
- expose `https://aerobag.org/cloud/` through nginx with SSE buffering disabled
  and an appropriate streaming timeout;
- harden and restart the daemon through systemd without recreating its data;
  and
- include ACS in deploy health and service reporting.

Production follows the same secret-delivery pattern as NMS credentials. The
deploy reads the operator-owned source
`/root/aerobag-credentials/aerobag-cloud-server-production.bin`, installs it as
`/etc/aerobag/secrets/aerobag-cloud-server.bin` with service-only permissions,
and passes that explicit path to `aerobag-cloud-serverd serve`. Operator
commands do not require this runtime HMAC secret. Rotating it invalidates
outstanding creation challenges and SSE tickets, but not stored account data.

## Pipeline Health

ACS exposes a small health endpoint and an operator status endpoint. Pipeline
health must not duplicate or guess ACS limits. The operator status response
reports configured warning, critical, and hard ceilings alongside current,
rolling, and peak observations. A generic bounded-resource shape is:

```text
name
current
peak_since_start
warning_at
critical_at
hard_limit
window_seconds       # for rates; absent for gauges
rejected_in_window
```

ACS reports this shape where applicable for:

- total account bytes, inline bytes, filesystem-blob bytes, pending-upload
  bytes, orphan bytes, SQLite bytes, WAL bytes, and filesystem free space;
- account count, object-row count, pending uploads, and retained SSE events;
- read, create, root-CAS, list, ingress-byte, and egress-byte rates;
- account-creation attempts, challenges, successes, and rejections;
- concurrent HTTP requests and current/peak SSE connections; and
- authentication, replay, malformed-request, quota, and rate-limit rejections.

GC additionally reports run count, last and peak SQLite write-lock pause,
cumulative write-lock pause, and last and peak total elapsed time. Every GC run
emits one low-frequency structured log line containing those timings and its
mark/delete counts. The checked-in policy supplies conservative initial
warning/critical thresholds; production-shaped measurement and threshold
validation remain an explicit TODO below.

The status response also includes bounded operator-only summaries of the
accounts and network pseudonyms currently responsible for the largest storage,
operation-rate, egress, connection, and rejection measurements. Account
locators and pseudonyms are opaque; ciphertext, secrets, raw IP addresses, and
request bodies remain absent.

Pipeline health collects and displays:

- process, database, schema, and WAL health;
- current use, configured limits, peak use, and remaining headroom for each
  bounded resource;
- global normal/read-only/suspended mode;
- account, object, and encrypted-byte counts;
- request, error, quota-rejection, and rate-limit rates;
- current and peak SSE connections;
- event publication, replay, cursor-reset, and delivery statistics;
- account-creation and challenge rates; and
- last successful durable read and write.

Pipeline health warns when a reported warning or critical threshold is crossed,
on an unreachable daemon, database failure, global read-only mode, abnormal
rejection or account-creation spikes, and event-delivery failure. Crossing any
global hard limit or rejecting work because a global resource is exhausted is
critical. Per-account quota rejection is visible and rate-checked but does not
automatically make the whole service critical.

Statistics include daemon start time and monotonic totals so pipeline health
can distinguish restart from counter reset. Rolling windows should include at
least one, five, and sixty minutes. Status collection must use maintained
counters and bounded/indexed queries; the monitoring endpoint must not itself
scan every object or become an abuse amplifier. A quiet server with no client
traffic is healthy and must not warn merely because timestamps are old.

## Implementation Order

### Phase 0: Preserve Existing Behavior

Status: complete on 2026-08-02.

- Capture the current Drive cross-device flows as regression tests.
- Extract the shared SSE heartbeat, idle-timeout, connection-timeout, and
  reconnect policy into a common Rust contract.
- Remove Kotlin and TypeScript timing policy; platforms execute a transport
  plan supplied by core.

Gate: Drive synchronization and live-feeds behave exactly as before, and a
static test rejects platform-owned SSE policy constants.

### Phase 1: Root And Wire Contracts

Status: complete on 2026-08-02. The contract review selected generic protobuf
AB3 and explicitly rejected AB2 compatibility.

- Introduce the narrow core root-publication abstraction.
- Keep Drive on its tested immutable-successor implementation.
- Define ACS account creation, signed request, object, root CAS, SSE ticket,
  event, status, and error contracts.
- Implement an in-memory ACS provider and deterministic race/failure tests.

Gate: review the versioned wire format, request-signature construction, KDF
labels, replay window, and Device Setup Code changes before exposing a network
service. Do not silently invent compatibility or migration behavior.

### Phase 2: Local ACS Daemon

Status: complete and reviewed on 2026-08-02.

- Implement SQLite metadata, inline objects, filesystem blobs, root CAS,
  visible authenticated references, and startup reconciliation.
- Implement signed authentication, anonymous account creation, `1 MiB` quota,
  operation/concurrency ceilings, SSE tickets, shared heartbeat policy, event
  replay, and bounded operator status.
- Implement server-owned mark-and-sweep GC and minimal operator commands for
  quota, read-only, suspension, and deletion.
- Hold an exclusive lifetime lock for `serve`, while allowing operator commands
  to use the same SQLite database; a second daemon must fail before processing
  requests because SSE fanout and runtime throttles are process-local.
- Add restart, race, quota, malformed-input, replay, GC, and monitoring tests.

The implementation lives in the standalone `services/` Rust workspace. The
initial configurable ceilings are a 1 MiB/2,048-object anonymous account,
600 authenticated operations, 32 MiB ingress, and 64 MiB egress per account per
minute; 1,200 operations, 64 MiB ingress, and 128 MiB egress per source-network
pseudonym per minute; 12,000 operations, 512 MiB ingress, and 1 GiB egress
service-wide per minute; and 4/16/128 concurrent SSE streams per
account/network/service. These are server policy defaults, not client
storage-format constants. Root ciphertext and visible graph metadata count
toward account and global stored bytes.

SQLite WAL owns metadata, inline ciphertext, quota reservations, fixed-root
CAS, nonces, tickets, and event history. Ciphertext over 128 KiB uses durably
renamed filesystem blobs. Startup reconciles interrupted reservations; the
daemon runs hourly mark-and-sweep with a 24-hour grace and removes aged orphan
blob files. Crossing the configured global stored-byte ceiling atomically
persists service read-only mode.

`serve` requires an explicit 32-byte `--server-secret` file outside the data
root. The store no longer creates or reads secret material alongside SQLite and
blob data. A `serve.lock` advisory lock under the data root enforces the current
single-daemon architecture without blocking short-lived operator commands.

Focused tests exercise the real HTTP router through signed account creation,
create-once object publication, root CAS, ticket creation, and SSE readiness;
they also cover concurrent CAS, nonce replay, retained-history reset, quota and
rate limits, interrupted blob restart, orphan GC, typed malformed/oversized
responses, persisted read-only mode, and bounded status output. CI now formats
and runs the standalone services workspace.

Gate: a local protocol test survives daemon termination at every publication
boundary, converges after CAS races, and alarms when injected load crosses each
reported threshold.

### Phase 3: Dev Stack And Web

Status: complete on 2026-08-02.

- Add ACS to `tools/run_dev_stack.py` at the documented persistent data path
  and `/cloud/` route.
- Add the core ACS adapter and thin web HTTP/SSE transport.
- Enable the existing Aerobag Cloud option and Device Setup Code flow.
- Add ACS status to pipeline-health.

Gate: two independent browser profiles exchange flight-plan and package-policy
edits promptly through SSE. The test must prove adoption happened before the
correctness poll and that a dropped stream recovers without lost state.

The automated two-profile browser gate creates and links an ACS Sync Account
through the real dev-stack HTTP daemon, then checks flight-plan and
offline-package-preference adoption. Its initial run observed 2.16-second and
0.97-second adoption respectively. It then forcibly dropped the receiving
browser's event stream, observed core acquire a distinct replacement stream,
and received a subsequent update in 1.01 seconds. The 20-second assertions are
strictly below the 60-second disconnected correctness poll.

### Phase 4: Android

Status: complete on 2026-08-03.

- Add only the Android HTTP/SSE transport effects required by the same
  core-supplied plans.
- Prove no ACS storage, retry, liveness, merge, or quota policy appears in
  Kotlin.

Gate: Android and web exchange both synchronized record types in each direction
and recover after disconnect/reconnect using the same core state machine.

Android now exposes the core HTTP and event-stream plans through the generated
JNI boundary. Kotlin supplies Google authorization, bounded HTTP execution, and
a blocking SSE reader on `Dispatchers.IO`; core still owns request signing,
URLs, retries, liveness, merge/adoption, and all persistent cloud state. The
Android pump serializes every resulting event back through the session boundary
instead of allowing transport coroutines to mutate a session concurrently.

The real dev-stack gate created an ACS account on Android, then proved both
flight-plan and offline-package-preference delivery from web to Android. It
interrupted Android's stream, observed a replacement connection, and delivered
a subsequent preference update over the replacement. Android then appended
`KSUS` and paused Terrain; a fresh isolated browser profile read both changes
from the shared account. This gate found and fixed two platform-boundary defects:
Android had stripped the core-required trailing slash from `/cloud/`, and
OkHttp requires an explicit zero-byte body for core's bodyless `POST`/`PUT`
plans. Both cases now have Android unit coverage.

### Phase 5: Production Readiness

Status: complete on 2026-08-04, excluding the separately authorized production
deployment.

- Extend `tools/deploy_prod.py` with the daemon build, systemd unit, persistent
  data/secrets paths, nginx SSE configuration, service reporting, and pipeline
  health wiring.
- Add coherent SQLite-plus-blob backup and restore tooling and exercise a
  restore in a disposable environment.
- Exercise account-creation, rate, storage, egress, SSE, disk-pressure, global
  read-only, and operator-containment limits.

Gate: deployment dry-run/config tests, restore test, and all pipeline-health
alarm tests pass. Preparing deployment is part of the MVP; actually deploying
or modifying production data and credentials requires a separate explicit
user instruction.

## MVP Stop Line

“Implement the plan” means complete Phases 0-5 through their gates, run the
dev-stack demonstrations and automated tests, prepare production deployment,
then stop. It does not authorize a production deployment.

The MVP includes:

- anonymous ACS accounts with the agreed small quota and abuse controls;
- native root CAS, SSE notification, rare correctness audit, and autonomous
  GC;
- inline and filesystem object placement;
- flight-plan and offline-package-preference synchronization;
- web and Android support;
- dev-stack, deployment definitions, backup/restore, operator containment, and
  pipeline-health alarms; and
- preserving the existing Drive provider.

The MVP explicitly excludes the work in `Deferred Work`. It also excludes a
rich admin UI, user-visible verified-identity enrollment, GPS trace upload,
PostgreSQL, S3, multiple daemon instances, unrelated cloud-record types, and
production execution. New requirements discovered while implementing a phase
must be added to this plan or discussed; they are not license to expand scope.

## Verification

- Provider contract tests prove atomic create-once, root CAS races, duplicate
  handling, ambiguous failure recovery, bounded reads, listing, deletion, and
  quota races.
- Model tests prove dropped, duplicated, delayed, replayed, and out-of-order SSE
  events cannot corrupt state or prevent later convergence.
- Restart tests kill ACS between object and event work and prove committed
  objects are announced after restart.
- Abuse tests cover quota races, request replay, malformed input, creation
  throttles, SSE limits, and global read-only transition.
- Monitoring tests drive each bounded resource across warning, critical, and
  hard limits and prove pipeline health reports the expected severity without
  exposing secrets or raw network identities.
- Dev-stack and production deployment tests verify persistent database paths,
  nginx streaming behavior, systemd restart, and pipeline-health alarms.
- Cross-platform E2E links two independent clients, modifies flight plan and
  offline-package preferences in each direction, and verifies prompt adoption
  without a poll.

## Completed Production-Readiness Slices

- **Rate limiting:** fixed process-local minute windows have been replaced with
  configurable, continuously refilling token buckets. Anonymous account
  creation atomically requires a durable per-network bucket (capacity `3`,
  refill `3/day`) and durable service-wide bucket (capacity `50`, refill
  `10/day`); only a genuinely new account commit consumes them. ACS reports a
  typed rejection gate and exact millisecond retry delay, while core owns the
  user-facing retry message shared by web and Android. Status reports
  daemon-lifetime and rolling one-, five-, and sixty-minute creation outcomes,
  including rejections by gate, without exposing network identities.
- A deterministic dev-stack fixture can exhaust either creation gate. The web
  E2E test verifies each message and status metric and emits screenshots at
  `/tmp/aerobag-cloud-rate-limit-network-ux.png` and
  `/tmp/aerobag-cloud-rate-limit-global-ux.png`.
- **Deployment wiring:** production source now builds the release ACS daemon,
  installs a hardened `aerobag-cloud-server.service`, keeps state under
  `/mnt/aerobag-data/cloud-storage`, installs the daemon secret outside that tree, and
  routes `/cloud/` through the host-local nginx with SSE buffering disabled.
  ACS is included in deploy health, pipeline-health dependencies, service
  lifecycle, and deployment config tests. This prepares deployment but does
  not authorize or perform it.
- **Proxy and operator boundary:** the public edge must overwrite
  `Aerobag-Client-Address`; host nginx accepts it only from the checked-in outer
  proxy address and overwrites it again for ACS. ACS honors that header only
  from its explicit loopback proxy allowlist. `/cloud/v1/health` is bounded and
  public; detailed `/cloud/v1/status` is hidden by nginx and independently
  restricted by ACS to direct loopback callers without a forwarded client and
  a bearer credential derived from the root-owned server secret. Detailed
  status and containment commands therefore share the existing host-login/root
  authorization boundary rather than adding a public admin API.
- **Runtime policy:** quotas, storage and body ceilings, concurrency, all token
  buckets, SSE limits, retained event count, GC schedule/retention, trusted
  proxies, and monitoring thresholds now come from one strict, versioned JSON
  policy consumed by both dev-stack and production. Unknown fields, invalid
  threshold ordering, and inconsistent ceilings fail startup.
- **Online backup and recovery:** ACS storage has an explicit
  `live/`, `snapshots/`, `recovery/`, and `locks/` layout. An hourly service
  holds a cross-process blob-reclamation lock, uses SQLite's online backup API
  from a short pinned WAL snapshot, then hard-links that snapshot's exact set
  of immutable ready blobs. Verification checks SQLite integrity plus database
  and blob hashes. Offline restore atomically replaces `live/` while preserving
  the previous tree under `recovery/`. GC is the only code allowed to unlink an
  installed blob generation.
- Production systemd and the dev-stack supervisor both invoke the same
  `backup-if-due` operation. The persisted due-time decision is serialized with
  backup creation; `backup-now` is the explicit operator/testing override.
- Backup age, total duration, SQLite snapshot duration, WAL growth, linked blob
  count, and linked blob bytes are status metrics with explicit warning and
  critical thresholds. Pipeline health evaluates those thresholds without a
  backup-specific side channel. Backup admission reserves enough free space
  for another complete SQLite snapshot plus the normal filesystem safety floor;
  failed and abandoned staging trees are removed so retries cannot fill the
  disk with partial database copies.
- **Global read-only recovery:** `set-mode normal` is not an operator command.
  `resume-writes` checks SQLite integrity, configured quota headroom, and free
  filesystem space. `force-resume-writes` is separately named, requires a
  reason, and records that reason in the operator audit table.
- **Production-shaped workload:** `aerobag-cloud-workload` drives the real HTTP
  router against disposable storage. Its hermetic profile crosses quota, SSE,
  egress, global-storage, filesystem-pressure, read-only, operator-status,
  online-backup, and GC boundaries, then feeds the resulting status snapshots
  through the actual pipeline-health evaluator in CI. Its release-mode
  production profile retains the deployed policy and reports stage latency and
  throughput falloff, SSE delivery latency, backup/WAL cost, GC pause time, and
  RSS. The initial 32-account/1,536-object/128-SSE run completed in 4.6 seconds;
  object-write p95 grew 1.47x across four stages, reads remained flat, online
  backup took 351 ms, and GC paused SQLite for 26 ms.

## Known Limitations And Follow-Up

These are known limitations from the Phase 2 review. They are deliberately
recorded rather than hidden behind compatibility behavior or guessed policy:

- **Large transfers:** the current 2 MiB HTTP body ceiling is correct for
  anonymous MVP accounts but is not a transport for future large GPS traces.
  Add a resumable large-object transport when the blessed large-object quota
  class is designed; do not force application-level chunking merely to bypass
  this limit.
- **GC at scale:** the current collector holds SQLite's in-process connection
  mutex and an immediate write transaction while traversing every account. The
  repeatable production-shaped baseline currently pauses SQLite for 26 ms over
  32 accounts and 1,536 objects, and CI proves pipeline-health classifies the
  configured thresholds. Continue watching `gc_database_pause_ms` and
  `gc_elapsed_ms` after deployment. If real growth approaches the warning,
  refactor marking into bounded read phases and short validated delete
  transactions rather than merely increasing an alarm threshold.

## Deferred Work

- Large GPS trace synchronization, resumable transfer, and its blessed-account
  quota class.
- Verified identity entitlements and payments.
- Multiple ACS instances and external event distribution.
- Drive successor-chain checkpointing, compaction, and client-driven garbage
  collection.
- Account migration between providers.
- Rich operator account tooling beyond containment primitives.
