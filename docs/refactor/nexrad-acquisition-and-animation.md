# NEXRAD Acquisition And Animation

Status: implemented.

## Problem

Core owns NEXRAD animation timing and frame selection, but it currently sees
different frame models on each platform:

- Web loads the retained live-feed history manifests and fetches selected tiles
  just in time.
- Android eagerly downloads the current `install_state` ZIP, persists it, and
  installs only that one state into the session.
- `nexrad_frame_candidates` chooses either the one installed Android state or
  the web history. Android therefore has one frame and cannot animate.

The Android behavior is not supposed to become web-style JIT fetching. Network
access may be brief or unreliable in flight. Android must use connectivity
eagerly to acquire complete NEXRAD states and continue displaying and animating
them after the network disappears.

## Contract

Core owns:

- The platform acquisition policy.
- The retained NEXRAD frame count and version set.
- Eager full-state request planning for durable clients.
- The web tile-cache retain set and visible-viewport prefetch plan.
- Frame manifests, ordering, animation phase, dwell timing, and blank timing.
- Mapping a selected tile to a typed resource source.
- GC decisions for obsolete durable live-feed versions.

Platforms provide effects only:

- Fetch an HTTP resource requested by core.
- Persist an immutable live-feed package blob and core metadata.
- Read a member from a persisted package blob.
- Delete durable versions core no longer retains.
- Draw decoded image bytes at core-provided geometry.
- Retain or release web image resources according to core's frame-version plan.
- Wake core at the absolute animation deadline core supplied.

## Acquisition Policies

`JitPublicResources` is the web policy:

- Load the retained version and state manifests.
- While the layer is visible, prefetch the displayed viewport's tiles for every
  retained animation frame.
- While the layer is hidden, fetch no tile bodies but retain already-loaded
  tiles whose frame versions remain in the animation window.
- Release tiles after core removes their frame version from that window.
- Never require a complete `install_state` package.

`DurableCompleteStates` selects Android's configurable policy. Core, not
Kotlin, applies the saved choices:

- Full-offline coverage loads retained version manifests, downloads each due
  frame's selected immutable install profile, persists it, and resolves tiles
  only from the matching package.
- Visible-area-only coverage reuses the JIT public-resource planner used by web,
  including its multi-frame viewport prefetch and retained-version GC plan.
- Full-offline cadence is selected independently for shown, hidden, and asleep
  conditions. Core enforces `shown >= hidden >= asleep` and chooses the current
  condition from core-owned map visibility and ownship-source sleep state.
- `Every update` fills the retained animation window. Decimated 10- and
  30-minute cadences fetch only the current frame when due; they do not recover
  intervening producer frames, because doing so would erase their bandwidth
  savings.
- Changes to visibility, sleep state, or settings wake the generic Android
  acquisition executor; Android merely copies and executes the new core
  directive.

The policies change resource acquisition, not NEXRAD interpretation or
animation.

## Android Settings And Profiles

The Android-only controls are projected as ordinary settings rows when the
platform advertises `DurableCompleteStates`. Their values live in
`SettingsPreferences` and in the cloud record `settings/nexrad_acquisition`.
Web neither displays nor accepts those actions.

The initial offline detail choices are named for extension rather than UI
wording:

| Profile        | Published base level |
|----------------|----------------------|
| `offline_0`    | `res0`               |
| `offline_low1` | `res1`               |

Version manifests advertise profile payload refs and byte counts. The
publisher puts only that profile's base level plus the authoritative
`manifest.json` in each package; it does not publish a second all-resolution
NEXRAD install package. After verifying the downloaded blob and authoritative
manifest hash, core deterministically derives all coarser overview levels,
writes a local render manifest, and persists the augmented package. The
authoritative manifest remains unchanged for state identity.

Core uses advertised package bytes and the roughly five-minute producer cadence
to project the selected full-offline rate in MiB/h. Viewport-only mode has no
estimate because it deliberately does not measure recent viewport transfer
behavior.

## Core Model

The common session catalog contains frame descriptors:

```text
NexradFrame {
    state_id
    observed_at_utc
    manifest
    backing
}

backing =
    PublicStateTiles
    DurablePackage { product, version, blob_sha256 }
```

Animation operates only on ordered `NexradFrame` values. It must not branch on
platform or on whether an old `nexrad_installed` field happens to be populated.

A selected tile becomes a typed resource request:

```text
PublicUrl { url }                         // web
LiveFeedPackageMember {
    product,
    version,
    blob_sha256,
    member_path,
}                                         // Android
```

Android's platform adapter reads that member from the durable package store.
It does not know NEXRAD semantics.

## Persistence

The durable live-feed cache retains multiple immutable versions when the
core-owned product driver requests it. NEXRAD retains the same newest-frame
tail used by animation. Other products may continue retaining one version.

Persisted package payloads remain on disk. Core keeps only:

- Installed summaries.
- State manifests.
- Package blob identities.
- The temporary bytes for a package that has not yet been durably committed.

After platform persistence succeeds, it acknowledges the version and core drops
the temporary package bytes. Startup validates one package at a time and drops
its bytes immediately, so core never retains the complete frame tail in memory.

## Verification

- A full-offline durable cache in `Every update` mode requests exactly the
  selected profile for the seven retained frames and excludes older history.
- A decimated full-offline cache requests only the current selected-profile
  package when due, never the intervening producer frames.
- A viewport-only cache requests retained state metadata and only the tile
  bodies at the core-selected resolution for the visible viewport.
- Its retained set survives restart without loading package blobs into core.
- A session installed from that retained set emits the same animation phases,
  frame order, age labels, and deadlines as the web/JIT catalog.
- Full-offline Android tile requests use `LiveFeedPackageMember`; viewport-only
  Android requests use the same `PublicUrl` plan as web.
- A package for one state cannot satisfy a tile from another state.
- Web NEXRAD tile requests remain `PublicUrl` and never require package members.
- A hidden web layer emits no tile fetches; re-enabling it reuses retained frame
  images and fetches only missing frame/viewport resources.
- Android decodes the absolute epoch deadline as a 64-bit value and schedules
  against that deadline rather than extending dwell time by fetch/decode time.
- Platform boundary tests reject NEXRAD-specific selection, retention, or
  animation policy in Kotlin or TypeScript.

## Implementation Order

1. Add the explicit acquisition capability and retained-version policy.
2. Make the durable cache retain and expose versioned NEXRAD descriptors.
3. Persist versioned live-feed resources and add generic package-member reads.
4. Replace the installed-vs-history session branch with one frame catalog.
5. Use core's absolute deadline on Android.
6. Add red/green core, FFI, Kotlin, TypeScript, and persistence tests.
