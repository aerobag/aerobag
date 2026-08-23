# Historical Live-Feed Handoff Note

This note used to describe the retired pre-live-feed handoff where web staged
rolling products from the static artifact snapshot.

That is no longer the current contract. Rolling products now use the live-feed
contract:

- discovery and invalidation: `/live-feeds/v3/events`
- immutable version metadata: `/live-feeds/v3/versions/<product>/<version>.json`
- full states: `/live-feeds/v3/states/<product>/<version>/...`
- deltas, when supported: `/live-feeds/v3/deltas/<product>/...`
- daemon publication ledger: `current.json` (not an application-client API)

Current implementation notes live in:

- [live-feed-sse-plan.md](live-feed-sse-plan.md)
- [live-feeds-daemon-migration.md](live-feeds-daemon-migration.md)

The old web staging route should not be used for new work.
