<!-- SPDX-FileCopyrightText: 2026 Aerobag contributors -->
<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Service contracts and server deployments

The application names a service contract, initially **C1**, instead of a server
release. Production URLs are `/contracts/C1/packages/` and
`/contracts/C1/live-feeds/`. Staging uses `/staging/contracts/C1/…`.
`/contracts/C1/contract.json` exposes the complete definition.

The initial C1 application requires one normal client release. Existing
applications keep their `/releases/<tag>/…` URLs and their existing sunset
deadlines. There is no migration alias. Release-specific package and live-feed
trees remain useful for inspecting an exact deployment.

## Routine operations

Stage this initial feature with the usual command:

```sh
tools/prod_manage.py --stage --watch
```

After a C1 client is in production, stage a compatible server fix with:

```sh
tools/prod_manage.py --stage --server-only --watch
tools/prod_manage.py --promote
```

Both cycle production and the live-feed daemon come from the new immutable
server tag. The production client's tag is pinned separately in the staging
commit as `client_tag`. Its APK, web assets, About page and download metadata
are copied without modification and checked against their original digests.
Client changes in the server revision are not shipped by this operation.
Missing original client artifacts must be restored before server-only staging;
the builder refuses to substitute a fresh rebuild for retained bytes.

A normal `--stage` builds a new application as well. A new application may
continue to implement C1. Promoting a deployment with the same named contract
retires the obsolete producer without adding a sunset deadline. Applications
on that contract stay supported. Changing the contract retains the outgoing
contract's server for the normal sunset period.

Staging web pages select the staging contract from their `/staging/` mount.
To test the APK against staging, select
`https://aerobag.org/staging/contracts/C1/packages/` as its package source;
its live-feed source follows that selection. The shipping default remains the
production contract URL. Restore that source after staging tests.

## Compatibility and qualification

`crates/product-contracts/contracts/service/C1.json` is immutable. The Rust
descriptor composes package/publication schemas, NAVDB requirements, complete
live-feed formats and these behavioral promises:

- Published resource identities are immutable; discovery commits a complete
  publication rather than partially replacing the current one.
- Catalog, snapshot and delta recovery tolerate provider replacement and
  reconnect; an upstream error is not a successful empty observation.
- Observation and validity timestamps retain their UTC meaning.
- NOTAM projections correspond to the publication's actual airport catalog.
  The runtime catalog fingerprint is checked during deployment and is not a
  constant baked into C1.
- Application support follows the contract through `bulletins-v2.json`.
  Authored notices still support exact application-version audiences.

Generated inventories and published descriptor history are checked in CI.
Both server executables export `--describe-service-contract`; release building
compares those compiled definitions with the retained client's definition.
New fields, encodings or incompatible meanings require a new contract name
and a client release. A matching descriptor alone does not prove behavior.

For an exact staged server-only tag, E2E qualification builds the original
client revision with test instrumentation, uses that revision's semantic
driver and journey harness, and runs against the candidate's server/fixture
code. `qualification.json` records both commits and the harness digest.
Ordinary main/PR builds do not inherit stale staging selections. These test
APKs are instrumented builds; the production APK is separately retained and
hash checked. Ordinary candidate unit/integration checks still exercise the
candidate source. A failed qualification blocks ordinary promotion.

## Activation, retention and rollback

The controller prepares independent staging publications and live-feed
instances. Each generated nginx configuration pins a contract's static
publication and live provider together. Old workers retain their old pair
during reload. Public checks verify the contract descriptor, publication and
live status after activation. A failed activation restores the previous
generation and reloads nginx before obsolete providers are retired.

Existing generation/drain retention protects rollback files for one hour.
To explicitly return to an earlier deployment, restore its production binding
(including its original `client_tag`, if present) in `deploy/releases.json`,
clear staging, commit/push the desired state, and run `--reconcile`. Do not move
an immutable tag or hand-edit `channel-current`. After the retention period,
reconciliation may need to rebuild the old server and publication before
reactivating it. Its recorded qualification must still pass the usual gates.

Client assets have independent storage under `client-builds/`. Previously
released C1 web chunks and downloads remain available while C1 is supported,
even after their producer deployments are removed. Once a contract is absent
from both channels, those assets receive a one-hour drain period before
collection. Rollback/reactivation cancels that retirement while the contract
is active again.
Preserve this directory when restoring or moving the artifact store; source
alone does not reproduce the exact signed APK and web build bytes.

Operational bulletin publication writes both the legacy release-support
document and the new contract-support document under the publication lock.
The existing publication-failure measurement covers either failure; health
checks verify both committed digests. Retiring a server revision alone does
not generate an application-update notice.
