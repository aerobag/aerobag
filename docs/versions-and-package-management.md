# Versions and Offline Package Management

## Versions

Current-artifacts is a suggestion / manifest about what's available on this
server. Maybe there should be an all-artifacts manifest that's getting appended
(or updated by GC).

Each artifact should be product(_optionalregion)_cycleordate_hash.extn
This way, if I roll a product inside a cycle, we can update the current-artifacts
manifest and new fetchers will fetch the fresh hashes, but offline users can
decide "no, the current version is fine with me, it's still the right cycle".

Current artifacts should publish version bundles:

datasets: [
	{
		contract: 1.1,
		packages: [
			database_2604_a8371039.had
			sectional_nw_2604_23e89a0.zip
		[,
	},
	{
		contract: 2.0,
		packages: [
			database_2604_10aabc09.had
			sectional_nw_2604_23e89a0.zip
		]
	}
]

A major version number indicates a breaking change. Each app binary will expect a particular
major version. Publishing multiple datasets lets us support older apps while advancing the
version number for version-2-aware apps.

The minor version number I think has no significance except perhaps for archaelogy/provenance.
If I make what I think is a non-breaking change to the preprocessor, we'll bump the minor
version so it's clear that happened. For example, if I make an aesthetic improvement to shaded
relief tile rendering, that's a minor version bump, because the app logic doesn't need to
know about it. Likewise adding fields to database json records that the old code will happily
ignore. I guess the minor version is useful in that App version 3.3 will be able to display
all the features of a dataset with contract 3.3. App 3.2 should not crash with a 3.3 dataset,
and app 3.3 should not crash with a 3.2 dataset.

The app offline downloader will fetch the top-level current-artifacts manifest, look inside
for a block that matches its major version, and ignore the rest. Then it'll decide what packages
to fetch from within there.

In online mode, we'll mount the unpacked trees on versioned URLs: /dataset/contract1.1/sectional_nw_2604_23e89a0/tiles/z/y/x/foo.webp

Notably, two contracts can share packages that don't change between revisions. Sectionals are
likely to stay identical even when the version rolls. The multi-dataset-version artifact manifest
allows this (as in the example above). For the online mode, we can probably do some redirection
or hardlinking or something to achieve the same economy.

## Offline package management

A user of the android app is going to want to specify what data to download and be confident
that, when they're at 6000 MSL over Poughkipsee with no internet, they'll be able to pull up
the approach plates they need. The trivial way to satisfy this is to send the app every package
at every cycle. However, that's ~10GB: it may be too much data for some users' devices, or more
likely, too much to squeeze through a crappy hotel wifi. So we need to give users a little control
over the package management process. Avare had very fined grained control, which maybe made sense
in the era of 32GB flash tablets, but it was very tedious. We're going to aim for something
a bit easier at the cost of precise control, at least in the common case.

One class of policy decision is: What should we bring *onto* the device, and when?
The second is: what should we garbage collect *off* of the device, and when?

Here is what I think the default rule will be:

- User selects a subset of regions. (If I never fly the east coast, save some bandwidth and download delay.)
- User selects a subset of products. (If I never fly IFR, don't bother with enroute charts or plates.)
The app idempotently decides what belongs on the device at any given timestamp t:
- select the main database plus every package that matches both the region and the package filter.
- select every cycle of those packages that hasn't expired yet. So if we're at the tail of a cycle,
we may select both, so that we'll have current charts now, and the upcoming charts ready to go
as soon as they become current (even if that happens airborne or while staying in a remote
area with poor Internet).
- any packages on the device that aren't in this selection are eligible for GC and should be deleted.

[TODO: figure out how to have the app switch seamlessly from one cycle to the next without restart?
Or just put up a big "RESTART REQUIRED" splash at 0000 UTC? Who knows how many stashed copies of
the first dataset we'll have to chase down if we don't just demand a restart. Restart required
in the soup on an approach, tho? It must be deferrable.]

The app can invoke unification at any time (idempotence). Any time it does, it grabs a fresh
current-artifacts and evaluates the criteria above. The common case is nothing happens until:
- a cycle is released, in which case we fetch the next cycle's packages and leave them
lying aronud, or
- a cycle expires, in which case we GC its packages. 28-day half-cycles might only clean up a
little stuff, since much of the data is 56-day data where the package is 'current' in both
half-cycles and hence not expired.

(We should write extensive tests around this.)

This scheme is much coarser than Avare's. That's a blessing because it's set-and-forget; the
app just ensures a steady supply of fresh charts. It may be limiting in a few cases:

- Some users might want to pick from the region x product matrix in a finer-grained way, rather
than just the intersection of selected rows and columns. I'm not worried about this; if the
day comes, we'll add an alternate UI, but it's pretty obvious how those
selections would parameterize the same algorithm above.

- Sometimes a user might want to control their downloads very carefully. The use case I'm
thinking of is that I'm staying in a crappy hotel in rural Texas with a 56 kilobaud modem as the
internet uplink. I want to fetch *only* the SC plates and enroute charts; if I wait for my
entire preferred matrix, I won't be able to leave until Thursday.

Avare has an affordance for this: each package has 3 UI states: "trash it", "I want it generally",
and "I want it NOW." If you select a few packages in the NOW state and click "download", it'll
fetch exactly those packages (and then reset them to "generally"). It won't do a bulk update
until you click "update", which then refreshes all of the packages with either "want" label.

I'm thinking we might get a similar behavior as follows. Each package has a
"pause" or "play" (active) UI bit. In the common case, all packages are "play". If a user needs
to do battle with a crappy uplink, they click a "pause all" button. Then they switch to the
advanced-mode (where they can select individual packages from the matrix) and they unpause
the three packages they need to get out of Texas. They click the same "sync" button they always
click; now the play/pause states are part of the selection criteria. ("pause" means "don't fetch
new versions of this package" but also "don't GC expired versions of this package".)
Later, when they land in a more civilized place with good wifi, they click "resume all", which
sets all the selected packages to "play." Because the advanced view can now be represented
by the simpler rows+columns presentation, they can now flop back to the default selection view,
and we're back in the preferred condition.

The discussion above is really about slow products (28/56-day products, durable products like
terrain). Fast products want some other UI and recommended polic(ies). Fast products are things
that, if you don't have them, don't fundamentally break the application:
TFRs, NEXRAD, METARs, NOTAMs, ADSB traffic.
[TODO clear UIs for when these layers are selected but unavailable or stale.]

I'm thinking that the UI for these is:
For each fast product, select whether it's:
- pause/play for download (media player icons)
- visible/invisible on displays (eyeball / closed eye icons)
If the product download state is "paused", we can still have it be visible, but
as it ages, the indicator will go from (green) "NEXRAD 2 min old" to (orange) "NEXRAD 10 min old"
to (red) "NEXRAD stale" (and we stop displaying it at the stale threshold). Each fast
product will have its own warning & stale threshholds.
- "pause" only affects proactive internet fetches. Passive receiving paths (like ADSB transponder
or stratux) never pause.

Each of the user preferences above:
- slow product row/col & advance-matrix select/play/pause states
- fast product play/pause, visible/invisible
...are part of the user*device state. We should store it durably across app crashes/restarts.
When we have a cloud account server, we should store it durably there, and (optionally) sync
it between devices.

For our first cut of this system, let's skip the Advanced (fine-grained matrix) package selection
stuff. In fact, we might be able to skip it forever. if we put pause/play buttons on the
rows and columns (regions and products), the user can "pause all" and then unpause exactly the
row+columns they want, and get the right effect for the example use case of "I just want these
three charts." So the first cut of the user state is actually {unselected, pause, play} for
each region and for each product.
