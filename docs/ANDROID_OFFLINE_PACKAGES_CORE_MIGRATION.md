## Problem

Android Offline Packages currently works, but too much package-management logic still lives in
`ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/MainActivity.kt`.

That violates the intended split:

- core owns package-management state, policy, and transitions
- Android domain adapters own HTTP/files/ZIP access and durable local storage
- Compose renders state and forwards user intents

The recent tombstone mistake was a symptom, not an isolated bug.

## Logic Still Living In Android UI

These behaviors are still being orchestrated in `MainActivity.kt`:

- package-library fetch policy
  - auto-refresh on tray open
  - one-hour staleness check
  - package-source URL invalidation
- library fetch execution
  - fetch `current_artifacts.json`
  - fetch historical discovery manifests
  - fetch referenced bundle manifests
- package-management reducer orchestration
  - initialize from cached library + installed inventory
  - reduce on region/product/clock clicks
- sync orchestration
  - execute fetch and GC actions
  - summarize warnings
  - replan after sync
- bootstrap gate orchestration
  - retry runtime init after refresh/sync
  - keep Offline Packages forced open until runtime is available
- some presentation shaping
  - source URL field
  - library-loading / library-error panel state

None of that belongs in Compose.

## Target Split

### Core Owns

- offline package-management state
- cached library metadata state
- refresh policy
- clock selection state
- region/product/core row state
- planner inputs and outputs
- sync intent/state/result summary
- corrupt/tombstoned artifact policy
- bootstrap gate state

### Android Domain Adapters Own

- fetch URL -> bytes / temp file
- list installed artifacts
- install artifact
- delete artifact
- persistent prefs/blob storage
- ZIP entry access

### Compose Owns

- panel rendering
- button presses / text edits / refresh / sync intents
- progress/error display from already-derived state

## Migration Steps

### 1. Define A Core-Owned Offline Packages Session

Add a core-owned state blob and reducer surface for Offline Packages that includes:

- library metadata state
- selection state
- planner state
- sync state
- bootstrap gate state

Inputs should be intents plus platform snapshots, not Compose-local state.

### 2. Move Library Refresh Policy Into Core

Core should decide:

- whether cached manifests are missing
- whether cached manifests are stale
- whether source URL changed
- which discovery/bundle manifests are active

Android should only perform the requested fetches and hand the JSON back.

### 3. Move Sync Orchestration Into Core

Core should decide:

- which artifacts to fetch
- which artifacts to GC
- how sync status is represented
- how warnings are summarized in UI state

Android should only execute fetch/install/delete operations and report results.

### 4. Move Corrupt Artifact Policy Out Of UI

Unreadable installed artifacts must not be handled in Compose state.

Core/package-management state should own:

- tombstones or equivalent poison-artifact state
- planner impact
- user-visible GC/refetch status

Android runtime/package adapters should only report the unreadable artifact event.

### 5. Leave Compose As A Dumb Renderer

After migration, `MainActivity.kt` should no longer own:

- offline package library cache
- refresh heuristics
- sync state machine
- planner reducer sequencing
- corrupt-package recovery policy

It should only:

- render a core-provided `OfflinePackagesUiState`
- forward intents
- execute requested platform operations

## Immediate Guardrail

Do not add more package-management policy to `MainActivity.kt`.

If new behavior needs state, it belongs either:

- in Rust core, or
- temporarily in an Android domain adapter if it is platform-specific I/O

but not in Compose/UI state.
