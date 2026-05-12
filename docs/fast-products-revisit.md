## Fast products:

Let's talk about correcting how we think about fast products.
Up until now, they've inherited almost everything from the slow (cycles & stable) pipeline.
A big chonky build, the client polls via current_artifacts, if the pointer moves, the
client fetches the updated bundles. We have hacky stuff in the clients to poll and refresh
the fast products, different on each client, eww.

Let's completely revisit how we do this stuff, and make a better **plan**.  Path I see:

How do we detect that upstream products have released updated data?
Probably polling, mostly. The NOTAM product has some fancy watch-shaped interface.
We may want some sort of adaptive polling that learns that product X is always updated
every five minutes, so we start polling 0.9*5*60 after the last publication time.

How do we advertise available products to clients? Presently, we couple the notion of
"family available" to "version available" via current-artifacts. Instead we should decouple
these: Some static "fast-products-manifest.json" will provide the URLs to the client-facing
endpoints for the fast products; that knowledge will last the life of the application session.

How does the client detect that new data is available? And how does the client retrieve the
new data? These might be coupled, because I suspect for some data streams (TFRs, NOTAMs, maybe
METARs) the changes might often be smallish, and re-sending the whole thing is inefficient.
(We care a little about efficiency because clients may be trying to sip data through a crappy
4G connection at 6,000 MSL.) Options:

1. Do what we do now, maybe per-product: have a distinguished URL; client polls it for changes
with an HTTP cache-invalidation query.

2. Hanging get: client keeps a hanging get open so the server can notify it right away on data,
reducing latency (important for weather!) and obviating wasteful frequent polling. If the hanging
get dies, we fall back to #1. (How would we know if it had died? A heartbeat? Is that as bad
as re-polling? :grimacing:). On invalidation, the client gets a whole copy of the new data. That
is, the GET responds with the data.

3. Watch (Adya-style) protocol: Client hangs on a GET, server and client have agreed what data
version thet client has. On new data, server transmits a delta that transforms client's data into
a newer version.

4. polling-delta: Client has version X. It polls the server for "newest-version-based-on-X".
If the server has version Z, and has a good delta, it might send Z-X. Otherwise it just sends Z.

Perhaps, before we build anything complicated, we should measure! we have a few hours of
historical fast-product data on root@aerobag-prod.iac.jonh.net. Let's go study each product:
- How frequently does the product change?
- How big is the delta between adjacent copies of the product? (Answering this may require
writing per-product diffs.)

We should also discuss how the client will manage these products.
- All the management should be in core. Whatever fancy thing we do, if we need help from web-
or android- to open an HTTPS connection or a websocket or a hanging get, fine. But the platform-
part should not have any idea which product it's helping with or what the payload is. All the
mechanism goes in core so it doesn't diverge between platforms.
- core needs a way to be woken up on invalidation (or an a timer, if we don't have invalidations
from the server), and a way to prompt the ui layer to repaint the new product.
- core should already manage which layers are visible. Core should have a way to control
which are being fetched (and we may surface that in the UI). That is, the user may not want
to waste bandwidth fetching NEXRAD they're not looking at -- or, they may want to keep that
data hot just in *case* they want to look at it.

