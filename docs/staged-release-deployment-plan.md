# Desired-State Staged Releases

## Goal

Aerobag releases must be built and exercised without disturbing the currently
published application. A release becomes production through an atomic channel
change, not by rebuilding files in place and restarting the only live-feed
daemon while cycle products are still being generated.

The target workflow has these properties:

- Release assignments are source-controlled desired state.
- A release name is an immutable annotated Git tag resolving to one commit.
- Web, Android, cycle products, and live feeds can be exercised through
  `https://aerobag.org/staging/` before promotion.
- Production remains available while a candidate takes hours to build or fails.
- Promotion and rollback reuse qualified artifacts and only change channel
  pointers and live-feed routing.
- Production, staging, and supported sunset releases share build cache and
  immutable package bytes.
- Garbage collection retains everything reachable from every served release,
  not merely the production discovery manifest.

## Desired State

Add a checked-in `deploy/releases.json` as the sole release-assignment input:

```json
{
  "schema_version": 1,
  "production": {
    "tag": "2026-08-15.1"
  },
  "staging": {
    "tag": "2026-08-22.1"
  },
  "sunset": [
    {
      "tag": "2026-07-18.1",
      "until_utc": "2026-09-15T00:00:00Z"
    }
  ]
}
```

`staging` may be `null`. Production is required. A release may not appear in
`sunset` while assigned to production or staging. The controller validates
timestamps and rejects unknown fields.

There is no separate stage/promote command language. The operator changes this
file, commits it, and runs `tools/deploy_prod`. The deployed controller compares
the desired document with observed production state and converges toward it.

### Release Naming

Before assigning a new staging release:

1. Choose an unused release name such as `2026-08-22.1`.
2. Verify that no tag, release-state record, or release-output directory uses
   that name.
3. Create an annotated Git tag at the candidate application commit.
4. Push the tag to the canonical repository.
5. In a later commit, assign that tag to `staging` in `deploy/releases.json`.

The configuration commit therefore normally follows the release tag by one
commit. This is intentional infrastructure-as-code behavior. Application
artifacts are built from the configured tag, not from the later commit running
the deployment controller.

Release tags are immutable. If a tag is deleted or resolves to a different tag
object or commit than production previously observed, reconciliation fails
loudly. A failed build remains associated with its original tag and can be
retried; the release name is never recycled.

## Desired And Observed State

The checked-in file describes only desired assignments. Prod maintains a
separate observed-state document containing at least:

- the desired-state Git commit last received;
- each release tag, annotated tag object, and resolved commit;
- immutable web, APK, binary, and publication artifact identities;
- build and qualification status;
- live-feed daemon instance and socket state;
- active and previous channel generations;
- draining daemon deadlines; and
- failures that prevented convergence.

Observed state is written transactionally. Reconciliation is idempotent: a
second run with the same desired state performs no rebuild, daemon restart, or
pointer change unless an incomplete operation needs to resume.

## Release And Channel Storage

Separate immutable release outputs, shared package publication, and generated
serving views:

```text
<artifact-root>/
  release-builds/
    2026-08-22.1-<commit>/
      release.json
      web/
      downloads/
        aerobag-android-<short-commit>.apk
        android-apk.json
      bin/
        aerobag-live-feedsd

  published/
    main-<commit>/...
    release-old-<commit>/...

  channel-generations/
    0042/
      production/
        web -> ../../../release-builds/2026-08-15.1-<commit>/web
        downloads -> ../../../release-builds/2026-08-15.1-<commit>/downloads
        packages/
          current_artifacts.json
          main-<commit> -> ../../../../published/main-<commit>
          release-old-<commit> -> ../../../../published/release-old-<commit>
      staging/
        web -> ../../../release-builds/2026-08-22.1-<commit>/web
        downloads -> ../../../release-builds/2026-08-22.1-<commit>/downloads
        packages/
          current_artifacts.json
          main-new-<commit> -> ../../../../published/main-new-<commit>
      releases/
        2026-08-22.1/
          packages/
            current_artifacts.json
            main-new-<commit> -> ../../../../../published/main-new-<commit>

  channel-current -> channel-generations/0042
```

The exact relative paths may change during implementation, but these invariants
must remain:

- `published/` contains shared immutable product trees.
- Production and staging have different physical `current_artifacts.json`
  files.
- Their artifact-root entries resolve through cheap symlinks to the same
  immutable product directories.
- A channel generation is complete and validated before `channel-current` is
  replaced atomically.
- The previous generation is retained for rollback and in-flight readers.

This avoids the invalid design in which `/packages/` and
`/staging/packages/` directly alias one physical directory and therefore cannot
have different discovery files. It also avoids a special nginx exception for
`current_artifacts.json`; each public package root is a normal, internally
consistent publication view.

## Public Routes

Nginx serves stable channel and release routes from `channel-current`:

```text
/                                  production web
/staging/                          staging web
/packages/                         production package view
/staging/packages/                 staging package view
/downloads/                        production APK metadata and file
/staging/downloads/                staging APK metadata and file
/releases/<tag>/packages/          durable package view for one app release
/live-feeds/<contract>/            production-compatible live-feed daemon
/staging/live-feeds/<contract>/    staging candidate daemon
/releases/<tag>/live-feeds/<contract>/
                                   daemon compatible with that release
```

The root production package manifest contains the production contract set plus
the distinct contract sets still required by sunset clients. The staging
manifest contains the candidate contract set and is never merged into the
production discovery file. This distinction is required when production and
staging use the same contract identifiers but contain different candidate
data; a single manifest correctly rejects duplicate contract sets.

## Web And Android Builds

Web and Android outputs must stop being built directly into the directories
currently served by nginx. Each release build writes a new immutable directory.

The same web build is served during staging and after promotion. It must use a
release-aware runtime configuration or release-scoped resource roots so that it
does not need to be rebuilt merely because its channel changed.

The signed APK for a release permanently uses release-scoped endpoints:

```text
https://aerobag.org/releases/<tag>/packages/
https://aerobag.org/releases/<tag>/live-feeds/<contract>/
```

Its package discovery file advances as new FAA cycles are built, while always
describing contracts compatible with that release. The immutable package files
remain shared under `published/`. The release-scoped live-feed route may be
repointed to a newer daemon implementing the same exact contract.

Consequently, the exact APK installed from staging is the APK linked from the
production About page after promotion. It retains the existing Android package
identity and signing key, so installing it is an upgrade and preserves user
data and downloaded packages.

## Cycle Publication

The current multi-version builder already supplies the important foundation:

- one isolated `build-product` invocation per Git ref;
- shared build and fetch caches;
- hard-linked publication package files;
- immutable per-build `product_artifacts.json` manifests;
- primary-first merge semantics; and
- a final merge and GC step.

Extend that design rather than creating another product pipeline:

1. Resolve every desired release tag to an immutable commit.
2. Build each unique commit required by production, staging, and sunset
   assignments independently against the shared cache.
3. Preserve each successful `product_artifacts.json` without changing a public
   channel.
4. Generate separate production, staging, and per-release discovery manifests
   into a temporary channel generation.
5. Validate all manifest references and public package members.
6. Atomically install the generation only after all required inputs for that
   channel are ready.
7. Run GC only after the new generation is active and all roots are registered.

`merge-current-artifacts` therefore needs an explicit output destination or a
lower-level API that builds a discovery list without writing the historical
fixed global alias. The shared `published/current_artifacts.json` ceases to be
the controlling root.

Production remains the primary version whose preprocessor defines merge and GC
semantics until a candidate is promoted. Staging code may build staging
products, but an unpromoted candidate must not control production GC.

Release build failures are independent failure domains. A broken staging build
does not block periodic production product refreshes. A failing sunset rebuild
does not erase its last successful manifest; production may continue serving
the previous valid sunset publication while reporting the failure.

## Live-Feed Instances

Staging uses a separate daemon even when production and staging declare the
same live-feed contract. The candidate may contain a behavioral regression that
does not require a wire-contract bump, and staging must exercise that exact
implementation.

Each candidate daemon uses its release binary and isolated runtime state:

```text
<runtime-root>/live-feeds/2026-08-15.1/live-feeds.sock
<runtime-root>/live-feeds/2026-08-22.1/live-feeds.sock
<artifact-root>/live-feeds/2026-08-15.1/...
<artifact-root>/live-feeds/2026-08-22.1/...
```

Scratch and mutable product state are isolated by release. Immutable upstream
fetch cache may be shared where its locking and cache contract permit it.

Before promotion:

```text
/live-feeds/v3/          -> production release socket
/staging/live-feeds/v3/  -> candidate release socket
```

Promotion points new production requests at the already-running candidate
daemon. Existing SSE connections remain attached to the old daemon. The old
daemon enters a draining state and is stopped after connections close or a
configured deadline expires.

After promotion, old and new clients using the same exact live-feed contract
may share the promoted daemon. If that is unsafe, the producer change was not
actually compatible and requires a new contract path. Different supported
contract paths retain distinct daemon implementations.

Prefer stable nginx configuration that resolves a channel's current Unix
socket through the generated channel view. Prove with a production-shaped test
that changing the channel symlink affects new requests while existing SSE
connections drain normally. If nginx does not safely resolve that arrangement,
generate an upstream include and use a validated graceful nginx reload as part
of the channel transaction.

## Qualification

Qualification has two tiers:

1. Source-level CI runs before or concurrently with the expensive release
   build. It includes Rust, web, Android, tooling, fixture, and platform tests.
2. Candidate-backed qualification runs after staging publication. It exercises
   the actual staged web build, staged package discovery and resources, staged
   live-feed daemon, and signed candidate APK where supported.

Store a generated `qualification.json` keyed by release tag, commit, artifact
hashes, test-suite commit, and completion time. A result is invalid if any
controlling artifact changes.

Candidate-backed tests must include at least:

- web startup and first useful paint from `/staging/`;
- exact static contract selection and resource fetching;
- live-feed current-state loading and SSE invalidation;
- representative flight-plan, chart, plate, inspection, and offline-package
  journeys;
- signed APK metadata and certificate verification; and
- compatibility selection for every production and sunset contract.

Manual inspection of `/staging/` and optional APK installation supplements
these tests. It is not the only gate. Production assignment is permitted only
for the exact release previously qualified as staging, unless an explicit
emergency override is separately designed and audited.

## Promotion And Rollback

Promotion is expressed by changing desired state, normally in one commit:

- set `production.tag` to the currently staged tag;
- set `staging` to `null` or to the next candidate; and
- add the previous production tag to `sunset` when compatibility service is
  still required.

The reconciler recognizes that the desired production release is already built,
qualified, published, and running. It constructs a new channel generation and
switches production without rebuilding.

Activation must be transactional:

1. Validate desired state and resolved immutable tags.
2. Verify qualification and every referenced artifact.
3. Verify the candidate daemon is healthy.
4. Materialize and validate the new channel generation.
5. Atomically switch the channel generation.
6. Confirm production health through the public interface.
7. Mark the old daemon and generation as draining/rollback candidates.
8. If post-switch validation fails, restore the prior generation and routing.

Rollback is another desired-state commit assigning the prior tag to production.
As long as its retained release view and compatible daemon remain available,
the controller performs another pointer change rather than a build.

## Garbage Collection

The current GC derives important roots from one
`published/current_artifacts.json`. That is insufficient once staging and
release-specific views exist.

GC must traverse an explicit root registry containing:

- the active production generation;
- the active staging generation, if any;
- every unexpired sunset release view;
- every durable per-release view promised to an installed APK;
- the previous generation retained for rollback;
- draining live-feed release state;
- active build leases and incomplete candidate outputs; and
- configured historical retention.

GC operates only after a complete channel generation and root registry have
been atomically published. A candidate build must never overwrite production
discovery as an intermediate step. Dry-run reporting should name which release
or channel retains each otherwise-collectable tree.

Sunsetting a release is a deliberate lifecycle transition. Expiry removes it
from production compatibility discovery and permits its daemon to stop, but
the policy for release-scoped APK package endpoints must be explicit before GC
may remove those resources.

## Reconciliation Algorithm

`tools/deploy_prod` remains the one operator command. Infrastructure bootstrap
and controller installation may remain internal phases, but release behavior is
driven only by `deploy/releases.json`.

For each invocation, the controller:

1. Transfers the repository and all required annotated tags to prod.
2. Loads and validates desired and observed state under a reconciliation lock.
3. Resolves every tag and rejects mutation of a previously observed release.
4. Plans required builds, qualifications, daemon starts, channel changes,
   drains, and GC without changing active production.
5. Builds missing releases independently and records resumable progress.
6. Refreshes cycle products for each release without crossing failure domains.
7. Starts and health-checks missing staging/live compatibility daemons.
8. Runs candidate-backed qualification when its exact inputs are ready.
9. Materializes the desired channel generation.
10. Activates only channel assignments whose gates pass.
11. Writes observed state and health reporting.
12. Runs channel-aware GC.

The deployment output must distinguish `converged`, `building`, `qualifying`,
`blocked`, and `failed`. A long candidate build may continue under systemd, but
production remains on its prior converged generation. Re-running the command
resumes or observes the same work rather than spawning a duplicate build.

## Required Tests

Add symptom-level tests for:

- desired-state schema and invariant rejection;
- annotated-tag resolution and tag mutation detection;
- idempotent no-op reconciliation;
- failed staging builds leaving production untouched;
- production product refresh while staging is broken;
- separate discovery files over shared immutable package bytes;
- production and staging using the same contract IDs with different contents;
- promotion performing no release rebuild;
- rollback performing no release rebuild;
- old and new SSE connections across a daemon promotion;
- concurrent distinct live-feed contracts;
- GC retaining production, staging, sunset, rollback, and build-lease roots;
- expired sunset cleanup;
- exact staged APK becoming the production download; and
- controller interruption and restart during every mutating phase.

At least one production-shaped end-to-end test should start two release daemons,
serve different production and staging package manifests, promote staging, and
prove that new web/APK requests switch while an established old SSE stream is
allowed to drain.

## Implementation Slices

1. **Desired-state planner**
   Add `deploy/releases.json`, schema validation, tag resolution, observed-state
   storage, reconciliation locking, and a read-only plan. Preserve current
   deployment behavior while the planner is tested.

2. **Isolated release builds**
   Build web, APK, binaries, and per-ref cycle publications into immutable
   release paths. Stop writing web/APK outputs into active serving directories.

3. **Channel package views**
   Add explicit-output discovery-manifest generation, production/staging/release
   symlink views, atomic channel generations, and public route tests.

4. **Candidate qualification**
   Run source CI and candidate-backed E2E, write artifact-bound qualification
   records, and expose status through production health/admin reporting.

5. **Concurrent live feeds**
   Add release-scoped daemon units and state, staging and release routing,
   health gates, promotion routing, and SSE draining.

6. **Promotion and rollback**
   Enable desired production changes only for qualified staged releases. Add
   transactional activation, public post-switch checks, and automatic rollback.

7. **Channel-aware GC**
   Replace the single-current-manifest root assumption with the explicit root
   registry and test release sunset and rollback retention.

8. **Retire in-place deployment**
   Remove the path that restarts the sole live-feed daemon, rebuilds the active
   web directory, and starts a product build against production discovery.
   `tools/deploy_prod` becomes an idempotent desired-state reconciler plus rare
   infrastructure bootstrap.

## Deferred Boundary

This plan does not define a second Aerobag Cloud Service data store for staging.
Until that is designed, staging clients may exercise the backward-compatible
production ACS contract, but release reconciliation must not apply an
incompatible ACS schema or storage migration merely because a candidate is
assigned to staging.
