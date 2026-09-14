# Service Notifications Plan

Status: implementation in progress; not deployed.
Date: 2026-09-13.

## Goal And MVP

Give users a durable, readable history of Aerobag service announcements and
release-support notices, with one aggregate unread entry in the existing
caution tray. Keep all client behavior in core and share read acknowledgements
between linked devices without requiring a cloud account to use the feature.

An operator must be able to publish an announcement in under a minute without
building, staging, qualifying, or deploying an application release. Connected,
active clients should learn about it promptly through their existing SSE
connection. Offline or sleeping clients discover it when they next connect.

The MVP includes:

- A versioned, cross-release public bulletin document and lightweight publisher.
- Typed release-support facts generated from activated release assignments.
- Plain-text operator announcements with explicit applicability and severity.
- One core-owned inbox, a permanent route to its history, and an aggregate
  unread-notifications entry in the caution tray.
- Durable local caching and independently synchronized read acknowledgements.
- Web and Android presentation, deployment/dev-stack wiring, and behavioral
  regression tests on both platforms.

Not in the MVP: forced modals, snoozing, arbitrary remote templates or HTML,
advertising, an administrative web editor, a new notification daemon, a new
cloud provider, or automatic conversion of every monitoring alarm into a
user-facing announcement.

## Existing Mechanisms To Reuse

- [Release reconciliation](../../tools/release_reconciler.py) owns production,
  staging, and sunset assignments. Do not create another release-policy list.
- [Production serving](../../tools/prod_deployment.py) exposes `/health.json`
  from `<data_root>/health/status.json`, outside `channel-current` and release
  directories. The producer atomically replaces that file.
- [Dev-stack](../../tools/run_dev_stack.py) has the same `/health.json` route
  and atomic-file publication pattern.
- [Data status](../../ui/core-rust/crates/app-core/src/data_status.rs) owns
  caution records, actions, severity, and hush behavior. Hushed records currently
  do not contribute to the launcher count. Service notices need a read/history
  lifecycle rather than another use of that session-local hush mechanism.
- Reuse core's existing local persistence, keyed cloud records, HTTP effects,
  scheduling, invalidation, and platform control-rendering mechanisms.

### Endpoint Decision

Keep `/health.json` at its existing address with its existing operator-facing
purpose. Do not move it, add an alias merely for symmetry, or make client
notifications depend on its deployment configuration and internal health data.

Use `/service/bulletins-v1.json` for the new public client contract. Generalize
only the small, useful publication machinery: validation, serialized writes,
durable atomic replacement, and serving independently of release lifetimes.
Do not build a generic service-management framework.

## Authority And Data Contract

There are two producers, but one published document, decoder, and notification
presentation model.

### Release Lifecycle Facts

Generate facts from successfully activated deployment state and its authoritative
sunset deadlines. A staged candidate is not the current production release.
A failed deployment must not announce an intended promotion as accomplished.

Publish exact release identities, channel assignments, support deadlines, and
update destinations. Retain enough retired-release information to explain an
old installation after its files and live-feed daemon are removed. Do not use
the filtered set of currently served releases as the entire historical record.

Core combines these facts with the installed build/release identity, selected
service, platform capabilities, and clock. It owns applicability, wording,
severity, relative times, and typed update/reload actions. Platforms execute
those actions only when the user requests them; never auto-reload in flight.

Do not order arbitrary build strings or guess that an unknown development build
is a retired production release. Ensure official builds provide the exact
release identity needed by the contract.

Release notifications have stable identities tied to the affected release and
lifecycle milestone. Support-ending and support-ended are distinct milestones;
the clock may advance between them while offline. Changing a deadline is a
substantive update. A countdown changing from four days to three is only a
core-rendered string change, never a document write or a new unread occurrence.

### Operator Announcements

Keep authored notices in a source-controlled data file, proposed
`deploy/service-notices.json`. It contains data, not executable templates.
Each notice defines:

- Stable notice ID and an explicit positive attention revision.
- Title and plain-text body.
- Severity using the established semantic status vocabulary.
- Publication time, optional effective/expiration times, and active/resolved
  lifecycle state.
- Typed applicability, such as all users of this publisher or specified
  releases, platforms, or affected services. No expression language.
- Optional explicit safe external link; typed application actions are limited
  to operations defined by the client contract.

Validation rejects duplicate IDs, invalid intervals, unsafe links, malformed
targeting, and oversized input. Establish bounded document, entry, and text
limits in the shared contract before implementing readers and writers.

The operator deliberately advances attention revision when an update merits
renewed attention, for example an earlier deadline or expanded affected scope.
Typographic edits may change the document revision without changing attention
revision. Never silently edit substantive safety advice as a cosmetic change.

### Envelope And Identity

The versioned envelope names a stable publisher identity, document revision,
publication time, release facts, and the retained announcement history.
Document revision is delivery/cache identity; attention revision is user-read
identity. They are not interchangeable.

Bind publisher identity to the configured service authority. A document cannot
select an unrelated publisher's acknowledgement namespace. Self-hosted services
and dev-stack must not collide with production notice identities.

Define this in the existing shared product-contract machinery. Keep published
versions immutable and update generated consumers/fixtures together. Unknown
or invalid contracts produce a diagnosed refresh failure while retaining the
last valid document; they do not masquerade as an empty inbox.

## Lightweight Operator Workflow

Operator commands (implemented):

```sh
tools/publish_notices.py --check deploy/service-notices.json
tools/publish_notices.py --dev-stack deploy/service-notices.json
tools/publish_notices.py --prod deploy/service-notices.json
```

The first rollout installs the Rust validator and creates the stable service
directory. Dev-stack does this on startup; production does it through normal
reconciliation. Later notice publication uses these commands without rebuilding
an app or restarting a daemon. `--root`, `--publisher`, and `--validator` allow an
isolated local trial. Publishing merges notices by ID; omission does not delete
history. Set `resolved: true` to move a notice into history. Use
`attention_revision` to deliberately ask users to read an updated notice again.

Private `publication-status.json` and `history/` live beside the document, but
HTTP exposes only `bulletins-v1.json`. Pipeline Health checks publisher telemetry,
the validated document digest, and activated release assignments. No alert is
based merely on an old publication date. Read receipts never go to this service.

Builds accept `AEROBAG_SERVICE_BULLETIN_URLS`, a comma-separated list of explicit
stable publisher URLs (at most four). Official builds use the configured public
service origin, regardless of staging/release paths. Local dev builds select the
shared dev-stack; hermetic test builds select their isolated fixture server.

The production command uses the existing authenticated SSH operator path, not
a new public write API. It validates, shows the target environment and changes,
requests confirmation, publishes, and verifies the served revision and digest.
Its success output includes the URL and published revision. A verification
failure reports uncertainty explicitly rather than claiming publication failed
before a retry overwrites newer state.

Record the source revision, input digest, and publication audit locally on the
server. Git history is useful, but pushing an app release or waiting for CI is
not a prerequisite for publishing validated notice data. A local checkout may
publish new notices without deploying that checkout's executable code.

The installed publisher and ordinary release activation call the same merge
and publication operation. Keep authored notices and generated release facts
as separate owned inputs. Serialize writers, detect stale operator previews,
and compose from the latest inputs so a concurrent promotion and announcement
cannot discard each other's changes.

Persist inputs and publication history outside release directories, proposed
under `<data_root>/service/`; expose only the intended public document, not the
whole directory. Use durable atomic replacement and recoverable publication
state so interruption leaves a complete old or new document. Reconciliation
must preserve operator-published notices, not replace them with whatever stale
copy happens to be in an application release checkout. Retry incomplete
release-fact publication from activated state and report it in monitoring.

## Delivery And Discovery

Serve the same document through production nginx and dev-stack, independently
of NAV contracts, cycle packages, release asset GC, and release-specific feeds.
Use compression and conditional reads with a content validator. A document
cache may be reused offline, but not treated as newly checked just because its
contents remain available locally.

Give core explicit stable service discovery information through the existing
bootstrap/configuration mechanism. Do not infer it by trimming `/staging/` or
`/releases/<tag>/` from arbitrary resource URLs. Respect independently selected
application/package and live-feed providers; notices must identify the service
they concern. Deduplicate when those selections refer to the same publisher.
No hidden fallback to the developer's server.

Existing live-feed SSE announces the bulletin revision as a change hint. All
supported release daemons observe the shared bulletin publication at runtime;
do not compile notices into each daemon or republish a divergent copy per
release. On stream connection, provide current revision as well as subsequent
changes, so a missed event cannot strand the client.

Use the existing daemon scheduling machinery to observe publication changes;
no dedicated notification process or second client SSE connection. Budget
propagation for an active connected client below one minute, normally seconds.
Never emit a hint before the corresponding complete document is readable.

Core also fetches independently on startup, relevant service changes, and
resume/reconnect when due. Add an infrequent conditional-read backstop using
the shared scheduler, with an initial proposed four-hour healthy interval.
Reuse existing bounded retry/backoff discipline for failures. SSE hints bypass
the healthy interval and coalesce to one latest-document fetch. Idle/sleep
policy prevents a new background firehose.

The under-one-minute goal applies to publication and active connected clients,
not offline clients or clients whose SSE is already gone. Independent startup
and backstop reads preserve eventual access after release retirement. Cached
deadlines continue to produce the correct lifecycle state without a fetch.

## Core-Owned Inbox And Presentation

Core owns parsing, applicability, ordering, read state, relative-time rendering,
severity, action IDs, page state, and all resulting invalidations. Keep document
ingestion and persistence off UI/input paths using the established runners.
Platforms provide generic IO, clock/capability inputs, and minimal presentation.

Service Notifications is the first, full-width section of Status, above the
responsive status-tile grid, not a separate Home destination. Its title row has
a fold triangle. Core initializes each Status visit expanded if relevant unread
items exist, collapsed otherwise. Manual folding lasts for that visit; reading
the last item must not fold away the body being read. Scrolling/re-rendering must
not replay the page-entry action. Order unread items first, then read/history;
use severity and publication time for stable ordering within those groups.
Core sends a dedicated history tone for read, resolved, and expired notices:
both platforms use the existing light-gray quiet background without a highlight
border. Info, Caution, and Warning use the same severity order as data status,
with Warning more severe than Caution.

The caution tray contributes at most one service-notifications record, with
text such as `3 unread service notifications`. Its tone is the worst relevant
unread severity. Its action opens Status and its notifications section, closing
the originating tray; it does not hush notifications.
The aggregate disappears when there are no relevant unread messages.

Opening a notice's readable body marks that exact attention revision read.
Opening a list does not automatically acknowledge bodies the user has not
opened. Provide `Mark all read` for the currently presented relevant batch.
Capture the exact notice/revision set when invoking that action: an arrival
or revision update during processing must remain unread. No `Dismiss forever`.

Reading an announcement is not resolving an operational fault. Reading about
NOTAM distribution quality clears its unread contribution, not an independently
modeled NOTAM quality fault. The same distinction applies to actual missing,
expired, or disconnected data. This feature does not invent duplicate faults
solely to make a read announcement keep alarming.

No automatic modal or application restart is part of this implementation.
If mandatory acknowledgement is added later, urgency and interruption must be
separate policy with explicit consideration of active navigation.

## Local Persistence And Cloud Acknowledgements

Persist the last valid bulletin document and read receipts locally. The inbox,
history, and acknowledgement action work with no network and no Sync Account.
Expose last successful check information so cached announcements are not a
claim that the publisher has just confirmed everything remains unchanged.

A read receipt identifies `(publisher, notice, attention revision)`. Give each
receipt its own keyed record; do not write one shared `last_read_at` watermark
or one whole-array replacement. A read receipt only records that this revision
was read, so concurrent receipts for different messages union without erasing
one another. Do not add unread/undo synchronization semantics in the MVP.

Use the existing encrypted core cloud sync and eager local application path.
Acknowledging locally never waits for cloud publication. Importing a receipt
immediately invalidates the inbox and aggregate caution projection, on both
platforms. Never send read receipts to the public bulletin publisher as telemetry.

Applicability is device-local even though receipts are shared. Updating one
tablet does not change another tablet's installed release or support state.
Two devices seeing the same applicable notice revision share its read receipt;
another milestone or attention revision remains independently unread.

Respect existing cloud-account schema/version and explicit-upgrade rules when
adding record types; do not silently upgrade an older account. Keep server and
provider plumbing unchanged unless the existing contract actually requires it.

Retain published read/resolved notices, not just notices previously fetched by
this device. Archive is bounded by explicit retention and size policy, never
silently dropped to fit a response. Initially fail an oversized publication
with operator guidance; do not build a paginated archival service in the MVP.
Defer receipt garbage collection until a safe retention contract exists.

## Rollout, Safety, And Monitoring

Install the endpoint, publisher, SSE hint support, and client decoder in the
initial feature rollout. Previously shipped clients without that code cannot
receive this UI retroactively. After rollout, ordinary announcements require
no release build or daemon restart.

Public delivery is read-only and contains no secrets, internal health payloads,
user identity, or application code. Use the existing trusted service transport
and operator authentication, bounded parsing, safe links, and narrow nginx
routes. No new unauthenticated publication endpoint.

Follow the shared telemetry-contract rules for publication failures, unreadable
or oversized documents, mismatches between activated releases and published
facts, and last publication status. Quiet publication age alone is not an
alarm: unchanged bulletins may legitimately stay unchanged for months. Make
the bulletin URL and publishing status reachable from existing admin tooling.

## Implementation Order

1. Define and review the bounded v1 bulletin contract, exact build/publisher
   identities, stable discovery input, and read-receipt representation. Separate
   attention identity from render time and delivery revision in the types.
2. Build the lightweight publisher and stable route for dev-stack/production.
   Integrate activated release facts, atomic concurrent publication, and runtime
   SSE hints. Prove updates require neither staging nor a daemon restart.
3. Build the core controller, persistence, fetch scheduling, deterministic
   release presentation, inbox, and aggregate caution projection. Add the
   shared receipt record through the established account-format discipline.
4. Render the core page/actions in web and Android using existing UI mechanisms.
   Connect generated typed effects and invalidations; no platform-side policy.
5. Run the shared behavioral suite and real web/Android journeys. Demonstrate
   publishing a notice, sharing a read receipt, and reaching retirement advice
   after removing a test release's assets and feed daemon.

## Required Regression Evidence

### Publication And Delivery

- Publish and resolve a notice without an app build, release qualification, or
  daemon restart; verify the exact public bytes and prompt connected delivery.
- Race promotion with manual publication; preserve both updates and reject
  stale previews. Inject failures around persistence and publication boundaries.
- Prove deployment of an older checkout cannot replace newer operator notices.
- Remove a retired release's feed and files; its client can still fetch the
  stable bulletin URL independently and display meaningful release advice.
- Lose, duplicate, reorder, and coalesce SSE hints; reconnect revision and
  conditional reads converge without repeated unread occurrences.
- Exercise timeouts, malformed/oversized responses, unknown contracts, idle,
  and offline startup. Retain usable cached state without blocking the app.

### Core Behavior And Persistence

- Cover current, staging, sunsetting, retired, rollback, and unknown dev builds;
  use exact identity rather than lexical version guesses.
- Advance a fake clock across relative-time changes and lifecycle boundaries.
  Only a new milestone or attention revision renews attention; renders do not.
- Read a notice offline, restart, reconnect, and propagate its receipt. Race
  different devices reading different notices and preserve both receipts.
- Race `Mark all read` with a new notice or revised notice; the unseen revision
  remains unread. Delayed old document delivery must not resurrect read state.
- Verify publisher separation, selected-service applicability, and one device
  upgrading without making another device's release current.
- Verify the aggregate count/tone and immediate invalidation on cloud receipt;
  operational data faults remain after reading related announcements.

### Real UI Journeys

- Publish into a hermetic test service; web and Android show the same core text,
  severity, and notice list, with only the intended update action difference.
- Physically open the aggregate entry, read a notice, mark a batch read, revisit
  history, and restart. Verify actual rendered badges and page contents.
- Link two isolated clients and demonstrate that reading on one clears the same
  notice on the other without another tap. Include web/Android cross-platform.
- Keep active navigation and the flight plan intact during every notification
  update, acknowledgement, outage, and recovery.
- Follow the existing deterministic semantic journey framework: observable
  completion conditions, bounded diagnostics, no fixed sleeps as correctness.

Record exact executed coverage and remaining gaps. Passing a source-pattern
test is not proof that a notice was delivered, visible, or acknowledged in UI.
