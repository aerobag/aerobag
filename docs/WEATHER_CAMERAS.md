# Weather Camera Inventory

Cycle builds combine the FAA's supported
[redistributable sites API](https://weathercams.faa.gov/api/redistributable/sites)
with the website's `/api/sites` inventory. The official API returns US sites;
the website also supplies Canadian sites. We take the union by site ID, with
official metadata taking precedence for shared sites.

## Getting Your Own Credential

To obtain API access for your own deployment, email a request to
**9-AJO-WCAM-ProgramOffice@faa.gov**. We requested redistribution access to the
Weather Camera System, signed the Memorandum of Understanding (MoU) they sent,
and received a bearer token along with the finalized agreement. Our MoU included
a commitment not to profit from the data; review the terms the FAA supplies for
your deployment.

The agreement and token belong outside the repository. Each deployment should
obtain its own credential. API documentation is at
<https://weathercams.faa.gov/api/docs/>. Missing-token build and deployment errors
include the contact address and point to this document.

Put the raw token in `/root/aerobag-credentials/faa-weathercams-token`, with mode
`0600`. A trailing newline is fine; do not include the `Bearer ` prefix. Override
the location with `AEROBAG_WEATHER_CAMERA_TOKEN_FILE` when building elsewhere.

The preprocessor sends `Authorization: Bearer <token>` to the official endpoint.
Request headers reach curl through stdin; they are excluded from command-line
arguments, request debug output, download provenance, and cache identities.
The token is never part of published packages or client configuration.

Production deployment copies the token to
`/etc/aerobag/secrets/faa-weathercams-token` with mode `0600`, and exports that
path to the cycle builder. The deployment configuration keys are
`weather_camera_token_file` (local source), `weather_camera_prod_token_file`
(remote destination), and `weather_camera_source` (default `combined`).

## Source Selection

Both sources are enabled by default. Requests run concurrently; failure of
either selected source fails the fetch rather than silently publishing a partial
inventory. For diagnosis, explicitly select a single source:

```sh
export AEROBAG_WEATHER_CAMERA_SOURCE=website
# Or, for only the US inventory:
export AEROBAG_WEATHER_CAMERA_SOURCE=official-api
```

`website` selects the undocumented `/api/sites` endpoint with its required
same-origin `Referer` header and does not read a token. For production, change
`weather_camera_source` in the deployment configuration and update the runtime
configuration. Restore `combined` to enable both again. The two endpoints have
distinct files and cache identities, and the ordered source list is included in
the vector-build fingerprint.

`AEROBAG_WEATHER_CAMERA_INVENTORY=/path/to/sites.json` overrides network ingestion
entirely. Fixture builds can use this without source credentials. The direct
`build-vectors --weather-camera-inventory` argument also accepts this envelope;
repeat the flag for multiple files in priority order.

## Coverage Monitoring

On 2026-09-07 the official inventory contained 756 US sites, and the website
contained those same 756 plus 218 Canadian sites. Their union was **974 sites**.

Pipeline health exposes **Weather camera sites** for each release and warns
when the published count is **below 960**. Exactly 960 is OK. Counts measure
unique sites, not individual cameras, images, or replicated vector tiles.
For overlapping cycles it displays the lowest count, with per-cycle details,
so a healthy cycle cannot hide a smaller inventory. Missing or invalid counts
also warn when the producer contract promises them. The metric is part of the
normal dashboard, history, and alert list.

The count travels from vector statistics through NAVDB build metadata into
`product-facts.json`. It therefore measures the package being served, without
making additional FAA requests on every health poll. A new cycle publication is
needed to reflect an upstream inventory change. Known older producer contracts
without this instrumentation show **Not instrumented**, with no camera-count
alarm. Rebuilding the same old producer does not add telemetry; a producer whose
contract promises the count warns if it is absent. These rules apply to every
channel, not only sunsets. See [producer telemetry contracts](../contracts/telemetry/README.md).

## Published Data

The vector builder validates the response envelope, inventory size, unique site
IDs, and coordinates, then emits the existing source-independent camera points
in airport vector tiles. It keeps site metadata and the public FAA site-page URL.
Current images bundled into the API response are not frozen into cycle packages;
the app continues opening the FAA camera page. Adding in-app live camera images
would need a separate acquisition path.

Unit tests cover source selection and union, missing and malformed tokens, authenticated
HTTP transport and cache replay, credential privacy, production installation,
conversion into camera points, and the coverage alert threshold.
