# Shared local documents

Core's local-document boundary replaces the former settings-only storage trait.
Settings/session/cloud state and the tour-introduction document are its initial
clients. Sharing this mechanism does not make a document cloud-synchronized.

## Ownership and completion

- Core owns keys, schemas, retention and synchronization policy.
- Hosts read opaque bytes and atomically replace or delete one document.
- A write is queued, not saved, until the host reports completion.
- Core permits one in-flight write per document and coalesces pending replacements.
  Different keys may complete out of order; older writes cannot overwrite newer
  values of the same key.
- Core keeps local edits after a storage failure, reports it through the existing
  status/caution model, and retries the latest document after 30 seconds.
- An unreadable existing settings document is protected from replacement. Local
  operation continues, but the user must repair storage and reload to recover it.
- A mutation advertises a near-term completion check through the existing
  snapshot scheduler. Storage completion does not wait for another user input.

Android reads during background startup and writes on a dedicated native IO
worker, outside the session mutex. Its generic file adapter uses explicit atomic
rename and sync, because Android AtomicFile.finishWrite can log a rename failure
without throwing. AtomicFile remains the reader for existing recovery files.

Web preloads core-requested keys through a generic worker/page request-response
bridge. Writes use the same acknowledged bridge; errors are never silently
discarded. Existing localStorage keys and UTF-8 text remain unchanged. Opaque
binary data uses a versioned encoding. The browser storage API itself is
synchronous on the page thread; serialization is in the core worker and storage
requests are not awaited by input actions. Larger or more frequent documents can
use another host backend without changing core's document contract.

## Scope

This is a document store, not a filesystem or a transactional database. Receiver
capture streams, package ZIPs, and the receiver-report SQLite database retain
their specialized IO and durability contracts. Learned station metadata can use
one bounded document, written only when metadata changes, once per ingestion
batch. Weather reports and spatial indexes do not belong in that document.

## Verification

Core tests exercise coalescing, out-of-order completion across keys, failed
writes, latest-value retry, deletion, restart and reverting an in-flight edit.
Session tests verify that failed storage preserves local edits and exposes a
warning. Web host tests inject denied reads/writes/deletes and delayed completion;
the ordinary WASM startup smoke crosses the actual Rust/JS boundary and restarts
after both successful and failed settings writes. Android tests exercise the real
private-file adapter, interrupted replacements, deletion and filesystem errors.
