# Android Platform Domain Smell Burn-Down

## Goal

Android platform code should provide OS-shaped services only:

- list files and directories
- read and write bytes
- fetch bytes from a URL when core asks for a typed resource
- persist small platform preferences
- render core-provided view models and dispatch core-provided actions

Android should not know package families, nav-db selection rules, region
taxonomy, chart product policy, validity policy, action semantics, or aviation
data interpretation. When a fix moves ownership into core, check whether web
already has a sane path to reuse. If the new core model is new, update web to
consume it in the same change so the platforms do not drift.

## Working Rule For Each Item

For each burn-down item:

- Identify whether web already solves the problem through a core-owned model.
- If web is sane, make Android consume that same model.
- If web has parallel platform logic, move the logic into core and update both
  platforms.
- If the item is purely rendering, keep only presentation-token-to-platform-draw
  mapping in platform code.
- Do not add legacy fallback paths. Delete the old platform-owned path once the
  core-owned path exists.

## Items

### 1. Remove Android nav-db validation policy

Status: completed.

Current smell:

- `OfflinePackagesPage.kt` special-cases package family `"nav-db"` after sync and
  opens `NavKvStore` to validate the installed file.

Target architecture:

- Core decides whether an installed artifact requires validation, what validator
  to run, and how validation failures affect package status.
- Android only reports that a download completed and exposes generic file bytes
  through the storage adapter.

Web parity question:

- Web does not install offline nav-db packages. If core gains a validation-result
  model, web should ignore or display it only through generic status/warning
  surfaces, not implement its own validator.

Verification:

- Syncing a nav-db package still rejects unreadable/incompatible nav-db content.
- Kotlin contains no `"nav-db"` branch for post-download validation.

Result:

- Removed Android nav-db open-status inspection and the installed-artifact
  health event path.
- Android opens the core-selected installed runtime through the generic
  `NavKvStore` handle only; core owns installed artifact acceptance/rejection.
- Web parity impact: none needed. Web does not install offline nav-db artifacts,
  and no new platform-specific model was introduced.

### 2. Remove Android nav-db status formatting

Status: completed.

Current smell:

- `OfflinePackagesPage.kt` formats `NAVDB 1: 2606 ok` by splitting package IDs
  and interpreting readability.

Target architecture:

- Core emits either a generic offline package status row or a nav-db status view
  model with already-derived labels and severity.
- Android renders labels/status exactly as supplied.

Web parity question:

- Data Status already renders core-provided status rows. Reuse that model if it
  is adequate; otherwise extend it and update web at the same time.

Verification:

- Kotlin does not split package IDs to recover cycles.
- Unsupported/wrong-contract installed nav-db still appears intelligibly in UI.

Result:

- Removed the Android `NAVDB n: cycle ok/bad` status formatter from Offline
  Packages.
- Installed data status now comes from core-owned offline/data-status models
  instead of Kotlin interpreting nav-db package identities.
- Web parity impact: none needed; web already consumes core data-status rows.

### 3. Move offline product options/catalog labels out of Android

Status: completed.

Current smell:

- `MainActivity.kt` hardcodes selectable products such as `sec`, `tac`,
  `terrain`, `tpp`, and `csup`.
- `OfflinePackagesPage.kt` maps core row IDs such as `nav-db`, `vectors`, `geo`,
  and `terrain` to display labels.

Target architecture:

- Core/package catalog emits the complete product/row model: IDs, labels,
  grouping, visibility, enabled state, request state, plan rows, and size rows.
- Android renders that model without knowing the product taxonomy.

Web parity question:

- Web currently has less offline package UI, but any shared catalog/status page
  should consume the same core labels. Do not introduce an Android-only catalog
  vocabulary.

Verification:

- Adding a new product family in publication metadata does not require a Kotlin
  code change for it to appear in Offline Packages.

Result:

- Removed Android offline product option fixtures and product/region ID lists
  from the offline package controller inputs.
- Core now derives offline product and region dimensions, row labels, and
  ordering from current publication/package metadata plus installed plan rows.
- Web parity impact: no web offline-package selector exists today; shared
  catalog labels now live in core for any future web surface.

### 4. Move region names and sort order out of Android

Status: completed.

Current smell:

- `NativeAppCoreAdapter.kt` hardcodes region IDs, display names, and sort order.

Target architecture:

- Core/catalog metadata owns region ID, display label, sort key, and any grouping
  policy.
- Android receives ordered rows/options.

Web parity question:

- Check whether web has region naming/sorting in TypeScript. If yes, delete the
  mirror and consume core/catalog metadata on both platforms.

Verification:

- Kotlin contains no `regionDisplayName`, `regionSortOrder`, or region enum
  switch for user-facing catalog order.

Result:

- Removed Android region-name and region-sort helpers.
- Core emits ordered offline package rows with labels already attached.
- Web parity impact: no TypeScript region mirror needed changes.

### 5. Remove chart-family and region enum mirrors from Android adapter

Status: completed.

Current smell:

- `NativeAppCoreAdapter.kt`, `MainActivity.kt`, and `Models.kt` translate product
  family strings into Android-side enums or label codes.

Target architecture:

- Core sends opaque IDs plus display labels and icon/style tokens.
- Android maps only icon/style tokens to drawables/theme values.
- If FFI needs strong typing, generate both Kotlin and TypeScript wire types from
  a core-owned schema instead of hand-maintaining mirrors.

Web parity question:

- Check whether web has the same family string switches. If web already consumes
  core icon/style tokens, make Android match. If not, introduce the tokens in
  core and update both clients.

Verification:

- Kotlin no longer has chart family switch statements except icon-token-to-asset
  rendering.

Result:

- Removed stale Android package/content inventory models and chart-family enum
  mirrors.
- Remaining Android string switches are presentation-token-to-drawable mappings,
  not package policy.
- Web parity impact: no new core contract was needed; Android was brought back
  to the existing core-token model.

### 6. Move map-selection action semantics into core

Status: completed, including analogous flight-plan row chart navigation.

Current smell:

- `MapExplorerPage.kt` branches on action IDs `"plates"` and `"csup"` and
  constructs `Plate:<airport>:Folder` / `Plate:<airport>:CSup`.

Target architecture:

- Core emits action records with typed effects, such as `select_page`,
  `select_chart`, or `perform_session_action`, with all target data included.
- Android dispatches the effect generically.

Web parity question:

- Web likely already handles similar inspector actions. If web constructs plate
  target strings itself, move the action effect model into core and update both
  platforms.

Verification:

- Adding a new inspector button does not require platform-specific action ID
  branching unless it is a genuinely platform-only effect.

Result:

- Core map-selection actions now carry typed navigation effects for plate-folder
  and chart-supplement targets; web and Android both consume that payload.
- While auditing analogous cases, flight-plan row `Plates`/`Show Plate`
  navigation was found to have the same string-ID smell. Core now emits typed
  row navigation effects for opening airport charts and exact plate targets;
  both clients consume those effects instead of branching on action IDs.
- Platform-specific ID branches remain only for UI-controller flows that open
  local editors/pickers (`insert_before`, `insert_after`, `add_airway`,
  `select_procedure`).

### 7. Move package display-name formatting out of Android models

Status: completed.

Current smell:

- `Models.kt` derives display names such as `NW_ENR_L` from package family IDs.

Target architecture:

- Core/catalog metadata owns package display labels.
- Android models carry IDs and render labels supplied by core.

Web parity question:

- Check whether web has package name derivation for display or debug output. If
  yes, replace both with the core/catalog label.

Verification:

- Kotlin does not uppercase or special-case package families for user-facing
  labels.

Result:

- Removed Android package display-name derivation from dead runtime models.
- Offline package rows display core/catalog-provided labels.
- Web parity impact: no TypeScript package-name derivation was found for this
  UI path.

### 8. Tighten vector feature rendering tokens

Status: completed.

Current smell:

- Android classifies rendered features by `styleClass`/`kind` strings such as
  `airport`, `nav`, `vor`, and `obstacle`.
- Obstacle colors are hardcoded in multiple Android rendering sites.

Target architecture:

- Core emits explicit presentation tokens for symbol family, label policy,
  selected/highlight state, and semantic color token.
- Android maps those tokens to local drawing primitives and theme colors.
- If feature geometry remains generic, token interpretation should still be
  shared by schema/generation, not ad hoc string matching.

Web parity question:

- Web likely has analogous symbol/style handling. If web’s renderer has cleaner
  tokens, make Android consume them. If both parse strings, introduce a
  core-owned presentation-token contract and update both renderers.

Verification:

- Airport, VOR, intersection, obstacle, selected-feature, and flight-plan
  highlighting still match web visually.
- Android has no duplicate hardcoded obstacle colors outside theme tokens.

Result:

- Core now emits explicit `symbol_kind` and `obstacle_tone` presentation tokens
  on vector symbol features.
- Android and web renderers consume those tokens instead of parsing
  `style_class`/`kind` to infer symbol family.
- Obstacle colors moved into shared theme tokens consumed by both platforms.

## Suggested Order

1. Nav-db validation/status, because those are closest to the bug that triggered
   this audit.
2. Offline product and region catalog ownership, because those prevent the next
   publication/product family regression.
3. Inspector action effects, because they are user-visible behavior and likely
   duplicated with web.
4. Chart/package family mirrors, because those are broader but may require wire
   type/schema work.
5. Rendering tokens/colors, because they are lower-risk UI drift rather than
   product-policy bugs.
