# Compatible Sunset Live-Feed Sharing

## Status and Goal

Implemented; local verification results are recorded below. Reduce
duplicate upstream polling by allowing sunset clients to use the production
live-feed daemon when compatibility is proven.
Keep each release's client and cycle products hermetic. Staging always uses its
own release binary, daemon, and mutable state, even when contracts match.

This deliberately trades exact client/producer release qualification for a
tested compatibility rule. It does not claim to detect behavioral bugs hidden
behind unchanged contract numbers.

## Operator Policy

Add a sunset-only `live_feeds` policy to `deploy/releases.json`:

```json
{
  "tag": "2026-09-11.2",
  "until_utc": "2026-09-25T00:00:00Z",
  "live_feeds": "share_if_compatible"
}
```

- `dedicated` preserves the current release-specific daemon behavior.
- `share_if_compatible` is the default and permits sharing; it does not assert
  compatibility. Promotion explicitly writes it on the newly sunset release.
- The controller chooses either production or a dedicated daemon and records
  why. A mismatch is an ordinary dedicated outcome, not a failed deployment.
- Reject this setting on production or staging. Never share through another
  sunset release; every shared binding resolves directly to production.
- Version the desired-state change explicitly and migrate existing entries to
  `share_if_compatible`. Promotion preserves existing policies and explicitly writes
  `share_if_compatible` for the newly sunset release. Operators can opt out.

## Compatibility Evidence

### Complete Wire Contracts

The Rust-owned `live-feed-compatibility.json` inventory describes client-visible
live-feed formats: discovery and SSE events, product manifests, snapshots,
deltas, NAVKV transport/encodings, and raster/tile formats. Cover every
registered product, including METAR, TAF, PIREP, NOTAM, TFR, NEXRAD and atmosphere
products. Identify currently implicit format assumptions before enabling sharing.

Inventories are generated from the constants/types used by producers and
decoders, without Python or platform copies of version numbers. The older
`client-data-contracts.json` export lists manifest schema 3 and NOTAM schema 7
only, which is insufficient. The new, separately versioned compatibility
descriptor covers all eight registered products and the optional service-bulletin
SSE hint. Bulletin documents remain independently served at the stable service
URL; their availability is not a prerequisite for weather sharing. Explicitly version
any required inventory shape changes. Do not bump a weather payload contract
merely because deployment now records more metadata.

First-pass eligibility requires exact equality of the complete live-feed
contract inventories. Do not infer compatibility from a common `/v3` prefix,
release age, matching NAVDB versions, or missing fields. More permissive
producer-superset rules are out of scope.

### Publication Requirements

For each immutable package publication, generate a versioned compatibility
sidecar bound to the exact release/client build and publication identity.
Include the client wire inventory and its NOTAM airport-catalog requirement.

Read `airport/notam-catalog` through the shared NAVKV directory reader from
every NAVDB bundle in that publication, matching the daemon's existing union
semantics. Validate the catalog schema and identifiers before hashing. Use
one Rust implementation to hash a versioned canonical representation of the
sorted, unique airport IDs and catalog schema. Include an airport count for
diagnostics, but never use the count as proof of equality.

The release's own daemon binary provides an offline `--describe-compatibility`
command. The controller verifies its executable and inventory against
`release.json`, invokes that command against the exact publication, and retains
the result under `state/live-feed-requirements/<tag>/<manifest-sha256>.json`.
This does not need FAA credentials, an upstream fetch, or a running daemon.

The requirement is the union across the publication's supported cycles, not
the full NAVDB package hash. Different charts or compressed page layouts must
not prevent sharing when the airport catalog is identical. Exact catalog
equality is intentional: even a production-only added airport causes dedicated
operation in this first version. Do not accumulate all historical catalogs
forever; retain evidence for the publications/generations actually supported.

### Actual Daemon Readiness

Expose a separately versioned, uncached deployment endpoint on the daemon's
direct listener, for example `/live-feeds/compatibility.json`. Report:

- executable build identity and a process-instance identity;
- compiled producer wire contracts and configured product availability;
- loaded catalog schema, fingerprint and airport count;
- startup publication identity and readiness of the corresponding projection.

Compute this from the catalog object actually passed to NOTAM processing, not
by rereading today's manifest when the endpoint is queried. Probe the direct
daemon endpoint, not a public alias that might already point elsewhere.

Persist catalog identity with the derived NOTAM projection. An old or unknown
identity requires rebuilding the derived projection from canonical NOTAM state
using the loaded catalog before advertising compatibility readiness. Do not
require an unrelated upstream NOTAM change to repair the projection. Cached
published products also need matching provenance; an HTTP listener and a new
in-memory fingerprint alone do not establish that served bytes match.

Process health and sharing eligibility are separate. A verified process with
warming NOTAM data, or with NMS disabled, can serve its own release normally;
it cannot qualify as a shared provider until the complete evidence is ready.

Keep startup manifests and their NAVDB inputs rooted in retention while a
daemon needs them, including automatic restart. Retain compatibility evidence
with its generation, independently of disposable publication directories.
Changing files on disk is not a catalog reload: changed launch inputs require
a prepared replacement instance and renewed readiness evidence.

Do not change frozen telemetry/status contracts in place. Any new monitored
facts must follow `contracts/telemetry/README.md` and update producer and rule
contracts together.

## Reconciliation and Lifecycle

Separate the release's public live-feed binding from daemon ownership. A
retained sunset release can still own client/package files without owning a
running daemon. Model daemon instances by release plus immutable launch inputs,
so a catalog update can prepare a replacement without mutating the provider
that existing bindings use.

For every proposed generation:

1. Resolve exact publication requirements and prepare the desired production
   instance. Keep staging independent and preserve exact-release qualification.
2. For each sunset, apply its policy and compare requirements to the ready
   production instance's actual descriptor. Unknown/incomplete metadata,
   unavailable required products, or any contract/catalog mismatch selects
   dedicated operation with a specific reason.
3. Prepare every required dedicated instance, including one previously stopped
   while sharing. A failure to prepare a required provider blocks activation;
   it must not silently authorize an incompatible route.
4. Validate the complete binding table against the same immutable publications
   and current instance identities immediately before activation. A changed
   observation invalidates the proposed decision. Serialize service/config
   changes under the existing reconciliation lock.
5. Activate channel data and validated nginx bindings through the existing
   generation transaction. Preserve rollback if validation or activation fails.
6. After the verified binding transaction is persisted, stop instances that no
   longer serve any binding. Retain their inputs/resources for the separate
   bounded GC grace period; never stop production because one sharing sunset
   expires. Retry interrupted shutdown or cleanup under the same lock.

Reevaluate on promotion, rollback, package/cycle publication changes, policy
changes, and daemon replacement/restart. In particular, prepare a sunset's own
daemon before a production change invalidates its sharing eligibility. Do not
turn an expected mismatch into either a promotion failure or an unsafe route;
only inability to prepare the required deployment is a deployment failure.

Observe and display the resolved provider, verification identities and reason,
for example `shared with 2026-09-13.2` or `dedicated: NOTAM catalog differs`.
Monitoring and retirement must operate on provider ownership, not merely on
retained release tags. Poll a shared provider once, attribute health to its
dependent releases, and do not report intentionally stopped redundant daemons
as failures. A `dedicated` policy is the explicit operational escape hatch.

Each generation retains `live-feed-bindings.json` with the resolved routes and
verification digests, plus `live-feed-evidence.json` with the exact publication
requirements and provider descriptor behind each decision. A later process
restart or publication refresh cannot overwrite this historical evidence.

## Routing and Client Cutover

Use internal nginx proxy routing, not cacheable HTTP redirects. Keep each
client's existing `/releases/<tag>/live-feeds/` prefix, production prefix and
staging prefix valid, including nested resource URLs. Do not chain aliases.

Outstanding HTTP responses may finish while routing is being verified. Once
activation commits, close the redundant provider, including its old SSE
connections, and let ordinary reconnect bootstrap from the new server's
catalog. Do not leave an old SSE feed issuing references against a new HTTP
provider throughout the one-hour file-retention grace. File retention is not
connection retention. No platform-specific sharing behavior is needed.

Equal schemas do not imply identical resource histories. Explicitly exercise
old events whose resources exist only on the old server, outstanding HTTP
responses, absent delta bases, and different NEXRAD history. A client must
resynchronize through the existing strict catalog/full-snapshot protocol and
reject obsolete completions, not guess that unavailable versions are equivalent.
Fix any uncovered recovery defect in shared core/lifecycle code with a failing
test first. No page reload, application restart or local-state reset is allowed.

## Test Plan

Use synthetic catalogs, compact generated products, independently seeded daemon
states, controllable clocks and explicit request barriers. No FAA credentials,
production files or uncontrolled network sleeps. Give independently actionable
cases separate test names and CI feedback.

| Area | Required cases and assertions |
| --- | --- |
| Contract inventory | Every registered product/codec is represented; exported metadata matches actual producer and client constants; changing each required schema/encoding independently denies sharing. Same `/v3` with different inner contracts fails. Missing, malformed or unknown descriptors never approve sharing. |
| Catalog identity | Order and cross-cycle duplication do not affect identity; union includes every bundle; addition, removal, equal-count substitution and schema changes differ. Different NAVDB package bytes with identical logical catalogs match. Invalid identifiers fail validation rather than being normalized into compatibility. |
| Runtime truth | Start with catalog A, then change the manifest to B: the running endpoint still reports A and B requirements fail. Restart with B changes identity. Readiness cannot precede projection/publication readiness. Retention preserves restart inputs and descriptors through publication GC. |
| Cached projection | Reuse persistent state built under A with catalog B, without a new upstream event. The resulting full product contains exactly B's intended projection before readiness. Unknown fingerprints, interrupted rebuilds and failures never relabel A's cached bytes as B or damage canonical source state. |
| Policy | Dedicated stays dedicated; compatible opted-in sunset shares; each mismatch/unknown stays dedicated with a useful reason; production/staging reject sunset policy. Desired-state migration and promotion preserve intended policies. |
| Lifecycle | Compatible promotion, incompatible promotion, rollback, cycle change, same-tag daemon replacement, explicit opt-out, multiple sunsets sharing, one sunset expiring, and production restart all produce correct bindings and daemon counts. Repeated reconciliation converges without start/stop churn. |
| Activation failure | Fail provider preparation, descriptor probe, nginx validation, activation, and controller execution between phases. Recover without publishing an unverified binding or deleting rollback inputs. Force an instance/publication change between comparison and activation to reject stale approval. |
| Ownership/GC/health | A shared sunset's redundant daemon can stop while its client/packages remain. Production remains while any binding needs it. Active/draining inputs are retained and eventually released. Health queries are deduplicated without hiding failures from dependent channels. |
| Real proxy cutover | Run independently seeded local daemons behind real nginx. Switch only sunset routing, retain a long-lived SSE connection, complete an old-only resource request across the switch, then force drain/reconnect. Verify correct prefixes, no redirect dependency, and staging isolation. |
| Core recovery | Script old-only snapshot/tile URLs, missing delta bases, late old responses and server catalog/history replacement. Require convergence on the new full state, subsequent updates and no mixing of incompatible resource versions. |
| Application journey | Keep a running client on the sunset URL; render identifiable old weather, switch providers without reload, and render identifiable new weather plus a subsequent update. Preserve flight plan, viewport and layer selections. Run on native Android and headless web; reuse Chrome-on-Android recovery coverage for its transport as well. |

The proxy test must exercise production routing/activation code rather than
merely compare generated strings. The app journey must assert real core-applied
data/rendered output, not just a green connection indicator. Prove its negative
control by suppressing reconnect/resynchronization, and retain failure evidence.

Put contract, catalog, projection and planner tests in existing cheap/ordinary
CI suites. Run compact real-proxy tests on relevant PRs and in release
qualification; declare nginx as a job dependency. Add the small shared cutover
journey to the existing E2E registry with explicit main/PR and release selection.
Reuse existing warmed fixtures/emulator ownership and do focused local runs
before hosted CI. Do not add another full prequalification round trip.

## Implementation Slices

1. **Evidence:** Centralize/export the complete inventory and catalog identity;
   generate publication sidecars and add fail-closed compatibility comparison.
   Add contract/catalog tests. No production routing changes.
2. **Runtime truth:** Add the daemon descriptor, projection provenance/readiness,
   pinned launch inputs and same-release replacement identity. Add deterministic
   startup/cache/restart tests. Keep all daemons dedicated.
3. **Bindings:** Add versioned policy, resolved ownership, transactional routing,
   drain/retention and operational explanations. Add planner/failure/ownership
   tests and the real-proxy cutover test.
4. **Qualification and rollout:** Add and run core recovery cases and the real
   client journey. Exercise compatible and incompatible transitions locally,
   then stage normally. Enable sharing only after exact runtime evidence exists.

## Local Verification

The focused Rust contract, producer/core interoperability, daemon provenance,
NOTAM projection, Python policy, lifecycle, monitoring and deployment suites
passed on September 13, 2026. Runtime's optional real-CLI integration also
passed using a freshly generated two-cycle NAVDB publication; ordinary Python
discovery skips that one case unless its two binary paths are supplied.

Five real-nginx tests exercise `Controller.activate` and crash recovery with
independent loopback daemons. They cover retained in-flight HTTP responses,
post-commit SSE disconnection, staging isolation, changed evidence, failed
activation, rollback and retained startup roots. Disabling retirement makes
the shutdown assertion fail. No production services are used by these tests.

The new `shared.live-feed-provider-cutover` journey passed on headless Chrome
and native Android. In both, suppressing replacement catalog/event delivery
fails specifically at the missing new rendered METAR count, after reconnection
succeeds. Restoring delivery renders old, new and subsequent weather while
preserving the flight plan, viewport, chart family and layer settings.
See [journey commands and evidence](../tools/e2e/live-feed-cutover.md).
Local screenshots and results are retained under
`/tmp/aerobag-cutover-evidence/final-{web,android}-{red,green}`.

Core additionally tests NEXRAD history replacement after a failed old request:
it installs distinguishable tile colors from the replacement history, rejects
a late prepared old tile package, and stops requesting removed versions.

All 13 suites in `tools/ci/cheap_preflight.py` passed locally. Preflight now
defaults to the checkout's private Cargo cache: the initial shared-cache run
exposed another checkout's schema-generator executable, not stale source in
this tree. Regenerating from this checkout confirmed the existing UI schemas.

The new journey has not been run in Chrome on Android. Existing browser-on-
Android transport recovery remains separate coverage. Full external fixture
replays, all release journeys and a production deployment have not been run
for this change.

## Initial Rollout

The 2026-09-13 audit found production `2026-09-13.2` and sunset `2026-09-11.2`
have equal current published airport catalogs: 19,452 IDs across cycles 2609
and 2610. Their examined live-feed contract implementations also match. This
is diagnostic evidence, not an automatic-sharing certificate: the running
processes have no loaded fingerprint and their startup manifests were removed.

Deploy reporting support in a new release; do not change historical release
binaries/tags or invent complete contracts for old clients. Pre-feature releases
without authoritative complete descriptors remain dedicated. Stage and verify
the new mechanism while dedicated, then exercise sharing when both sides have
the required evidence. Sharing then takes effect automatically under the
default sunset policy; `dedicated` remains the explicit opt-out. No manual
hash allowlist or compatibility bypass is part of this implementation.
