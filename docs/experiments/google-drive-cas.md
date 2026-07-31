# Google Drive CAS Experiment

## Purpose

This experiment determines whether Google Drive can provide the conditional
root update required by Aerobag's encrypted cloud-storage design.

The browser lab is available at:

```text
/experiments/drive-cas
```

It creates one dedicated file named `aerobag-drive-cas-lab-v1` in the user's
Google Drive `appDataFolder`. The lab never requests access to or modifies
normal Drive files.

## Google OAuth Setup

Create a Google OAuth client of type `Web application`.

Google requires HTTPS for non-localhost JavaScript origins. The simplest
development setup is to forward the Vite port from the machine running the
browser:

```text
ssh -L 8083:127.0.0.1:8083 aerobag-dev.iac.jonh.net
```

Then open:

```text
http://localhost:8083/experiments/drive-cas
```

Add the exact origin `http://localhost:8083` as an authorized JavaScript
origin. Do not include a path. Localhost is Google's explicit HTTP exception.
Alternatively, expose Vite through a trusted HTTPS reverse proxy and register
that HTTPS origin.

If the OAuth application is in testing mode, add the Google account running the
experiment as a test user.

The client ID may be pasted into the lab or supplied when starting Vite:

```text
AEROBAG_GOOGLE_DRIVE_EXPERIMENT_CLIENT_ID=...apps.googleusercontent.com
```

No Google client secret belongs in Vite, browser storage, or the experiment.
The lab requests only:

```text
https://www.googleapis.com/auth/drive.appdata
```

The OAuth client ID and lab file ID are retained in local storage. The access
token remains in memory.

## Procedure

1. Open `/experiments/drive-cas`.
2. Enter the OAuth web client ID.
3. Click `Authorize Drive lab`.
4. Click `Create fresh lab object`.
5. Run the default three races per upload mode.
6. Download the JSON evidence.
7. Increase the iteration count before drawing a final conclusion.
8. Delete the lab object when testing is complete.

The lab exercises:

- simple media updates;
- multipart media updates; and
- resumable updates.

For each mode it first submits a deliberately stale `If-Match` condition. It
then repeatedly makes two concurrent updates based on the same observed
condition.

The resumable test establishes both upload sessions before allowing either
session to upload its body. This detects a provider that validates the old
revision only when creating a resumable session.

## Verdicts

`CAS OBSERVED` means:

- the deliberately stale condition was rejected;
- exactly one writer succeeded in every race;
- the other writer received a conflict response; and
- the final file content belongs to the successful writer.

`UNSAFE` means a stale write succeeded or two writers based on the same
condition both reported success.

`INCONCLUSIVE` means the browser could not obtain a usable condition token, a
transport failure substituted for exclusion, both writers failed, or the final
content did not agree with the apparent winner.

The lab uses an HTTP `ETag` when Drive exposes one to browser JavaScript.
Otherwise it experimentally tries the quoted Drive file `version`. A quoted
version is not assumed to be valid merely because Drive reports it.

## Result: Conditional Mutation Is Unsafe

The 2026-07-31 run used three stale-condition checks and three concurrent races
for each upload mode.

- Drive exposed no HTTP `ETag` on metadata reads, media reads, or writes.
- A deliberately invalid quoted `version` in `If-Match` was accepted with
  `200 OK` for simple, multipart, and resumable uploads.
- Both competing writers returned `200 OK` in all nine races.
- For resumable uploads, both session starts and both session finishes returned
  `200 OK`.
- The final writer varied, confirming last-completion-wins behavior rather than
  deterministic exclusion.

Therefore the Drive adapter cannot implement Aerobag's conditional mutation
contract using `If-Match`, Drive file `version`, or resumable-session creation.
The reported `version`, checksums, and revision IDs are useful observations,
not write preconditions.

Drive documents a different atomic-looking primitive: create using a
pre-generated file ID, where a repeated create returns `409 Conflict`. A
separate experiment must verify concurrent create-once behavior before
considering a lease chain built from pre-generated IDs. Such a design must
also recover from a client that acquires a lease and crashes before publishing
the next root.

## Generated-ID Create-Once Follow-Up

The same browser lab contains a separate `Run create-once experiment` action.
It does not reuse the mutable CAS report or verdict.

For each run the lab:

1. obtains fresh file IDs from `files.generateIds` for `appDataFolder`;
2. races two multipart creates carrying the same generated ID;
3. reads the resulting object to identify the committed writer;
4. retries an already-successful create using the same ID;
5. deletes a separate created object and tries to create that ID again; and
6. removes test objects that remain after the experiment.

`ATOMIC CREATE-ONCE OBSERVED` requires exactly one successful writer in every
race and a conflict when retrying an already-created ID. Delete-then-recreate
is reported separately because it determines whether an ID can act as a
reusable lock or only as one link in a lease chain.

### Result: Atomic Create-Once Observed

The 2026-07-31 run completed three concurrent races:

- every race had exactly one `200 OK` winner and one `409 Conflict` loser;
- reading the created object always returned the successful writer's payload;
- retrying a completed create returned `409 Conflict` with
  `reason: fileIdInUse`;
- all cleanup deletes returned `204 No Content`; and
- recreating a deleted generated ID returned `400 Invalid Value` for
  `fileId`.

The generated ID is therefore an atomic, permanently single-use slot. It
cannot implement a reusable lease by deleting and reacquiring the same ID.

A better protocol can avoid leases entirely. Each immutable state node carries
the generated ID reserved for its successor. Concurrent clients race to create
that successor object, including the complete new state-root reference and a
fresh successor ID in the same atomic create. Exactly one client wins. A crash
before the create commits publishes nothing; a crash after it commits leaves a
complete next node. An ambiguous result is resolved by reading the reserved ID:
the object either contains the client's proposal or the competing winner.

This creates an append-only state chain rather than a mutable CAS root. A
best-effort mutable head hint may accelerate startup, but correctness must come
from the immutable chain. The remaining design work is bounded history,
checkpointing, and garbage collection; chain IDs cannot be deleted and reused.

## Limitations And Follow-Up

The failed stale-condition checks are sufficient to reject conditional
mutation; more races cannot qualify it. Provider work should continue only
through a different primitive, beginning with pre-generated-ID create-once.
