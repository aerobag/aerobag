# Historical Live-Feed Handoff Note

This note used to describe the retired pre-live-feed handoff where web staged
rolling products from the static artifact snapshot.

That is no longer the current contract. Rolling products now use the live-feed
contract:

- discovery: `/live-feeds/v2/current.json`
- immutable version metadata: `/live-feeds/v2/versions/<product>/<version>.json`
- full states: `/live-feeds/v2/states/<product>/<version>/...`
- deltas, when supported: `/live-feeds/v2/deltas/<product>/...`
- invalidation: `/live-feeds/v2/events`

Current implementation notes live in:

- [live-feed-sse-plan.md](live-feed-sse-plan.md)
- [live-feeds-daemon-migration.md](live-feeds-daemon-migration.md)

The old web staging route should not be used for new work.
