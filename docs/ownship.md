Rules of engagement: no logic goes in the UI, only display stuff. Logic
that ends up there diverges between the two platforms, and that's a bug
farm.

Here are my goals:

When you're flying with the android tablet offline, you'd like to gather
ownship information and drive the display.
Situation information might include:
	GPS location
	Heading
	Speed
	Altitude
	UTC timestamp of the event time

We can gather some version of all that from the GPS in a tablet. Maybe even baro
altitude from some phones.

We might also want to replace the tablet's local devices with inputs drawn from
an ARINC bus or a stratux with an AHARS. If we have an AHARS, it'll fill in
a complete Situation record; 

Of course, we also want to play with this thing in testing and demos, including
on the web. That will motivate having simulated inputs. Here, I'm thinking a
GPX track and a little ui widget that lets you pause/play/rewind/jog/set speed.

And where will we get GPX tracks? Well, one source might be something that fetches
them from adsb.fi or another adsb aggregator, so you're literally following some
plane's live or historical path. (Live would mean a different path that skips the
UI jog control, obviously.)

So let's think about the architecture.
1. Core will define a situation struct (it may already), and keep track of the
current situation.
2. When core learns an update, it'll send an event to UI, so UI can update ownship
representation on the CHT or georeffed PLT. Core will also compute updated
plan sequencing and CDI indicators and send UI update events for those.
3. Core might send situation history to a cloud server as it arrives and/or
collect situation history in a file and sync it to the cloud after a flight or
as internet is available.

And then there will be a way to select / prioritize among the possible input modes.
Core will provide UI with menu constructors to choose/sort among device GPS,
AHARS/external GPS, playback from jog, or maybe even a simpler FP sequencer (another
jog-like widget that lets us move the active leg up and down the FP, and we just set
the situation to be the position and heading of the beginning or middle of the leg).

I like most of your reply. An amplification and a clarification:

"never silently replace" -- yes, we need one more Situation, which is "None": no valid
source in recent enough history. So if your AHARS is gone and the tablet GPS doesn't
have a signal, we don't fall back to simulation, we switch to None. That triggers
painting a loud red warning banner ("NO GPS POSITION"), the ownship UI widgets vanish (but you
can still scroll around and read charts, of course), course guidance CDI disappears, no sequencing happens.

  - events/snapshots out of core:                                               
      - ownship_state_changed
      - source_status_changed           
      - guidance_changed                                                        
      - history_recorded                                                        
I don't understand the plan here. Core->ui should only be the events that tell the UI
to do very stupid painting:
	"2. When core learns an update, it'll send an event to UI, so UI can update ownship
	representation on the CHT or georeffed PLT. Core will also compute updated
	plan sequencing and CDI indicators and send UI update events for those."
For sending recordings to cloud, core can manage that network connection itself!
(Although we may need info about network availability from the platform.)
