x Rules of engagement: no logic goes in the UI, only display stuff. Logic
x that ends up there diverges between the two platforms, and that's a bug
x farm.
x 
x Here are my goals:
x 
x When you're flying with the android tablet offline, you'd like to gather
x ownship information and drive the display.
x Situation information might include:
x 	GPS location
x 	Heading
x 	Speed
x 	Altitude
x 	UTC timestamp of the event time
x 
x We can gather some version of all that from the GPS in a tablet. Maybe even baro
x altitude from some phones.
x 
x We might also want to replace the tablet's local devices with inputs drawn from
x an ARINC bus or a stratux with an AHARS. If we have an AHARS, it'll fill in
x a complete Situation record; 
x 
x Of course, we also want to play with this thing in testing and demos, including
x on the web. That will motivate having simulated inputs. Here, I'm thinking a
x GPX track and a little ui widget that lets you pause/play/rewind/jog/set speed.
x 
x And where will we get GPX tracks? Well, one source might be something that fetches
x them from adsb.fi or another adsb aggregator, so you're literally following some
x plane's live or historical path. (Live would mean a different path that skips the
x UI jog control, obviously.)
x 
x So let's think about the architecture.
x 1. Core will define a situation struct (it may already), and keep track of the
x current situation.
x 2. When core learns an update, it'll send an event to UI, so UI can update ownship
x representation on the CHT or georeffed PLT. Core will also compute updated
x plan sequencing and CDI indicators and send UI update events for those.
x 3. Core might send situation history to a cloud server as it arrives and/or
x collect situation history in a file and sync it to the cloud after a flight or
x as internet is available.
x 
x And then there will be a way to select / prioritize among the possible input modes.
x Core will provide UI with menu constructors to choose/sort among device GPS,
x AHARS/external GPS, playback from jog, or maybe even a simpler FP sequencer (another
x jog-like widget that lets us move the active leg up and down the FP, and we just set
x the situation to be the position and heading of the beginning or middle of the leg).
x 
x I like most of your reply. An amplification and a clarification:
x 
x "never silently replace" -- yes, we need one more Situation, which is "None": no valid
x source in recent enough history. So if your AHARS is gone and the tablet GPS doesn't
x have a signal, we don't fall back to simulation, we switch to None. That triggers
x painting a loud red warning banner ("NO GPS POSITION"), the ownship UI widgets vanish (but you
x can still scroll around and read charts, of course), course guidance CDI disappears, no sequencing happens.
x 
x   - events/snapshots out of core:                                               
x       - ownship_state_changed
x       - source_status_changed           
x       - guidance_changed                                                        
x       - history_recorded                                                        
x I don't understand the plan here. Core->ui should only be the events that tell the UI
x to do very stupid painting:
x 	"2. When core learns an update, it'll send an event to UI, so UI can update ownship
x 	representation on the CHT or georeffed PLT. Core will also compute updated
x 	plan sequencing and CDI indicators and send UI update events for those."
x For sending recordings to cloud, core can manage that network connection itself!
x (Although we may need info about network availability from the platform.)


x   Okay, it's up! Next step is to develop a source of simulated data to play
x   back, a way to select that data, and a jog widget to control it in the UI.
x 
x 
x   Next, let's work on having the chart automatically center on ownship.
x I think it works like this (although perhaps you have opinions).
x There's a center-here button on the CHT display (1x1, lower right. Move the playback tool to the lower left. move the debug button to the lower right, inset one from the center-here button.
x When the user presses center-here, the map centers on the ownship position.
x Center-here is only enabled if ownship position is known.
x (core computes enablement and tells ui. Core does all the thinking here, as usual.)
x 
x The core maintains a bit of state about whether we are "following" or not. Pressing
x center-here engages the following mode. Scrolling the map so far that the ownship leaves the
x viewport (a condition the ui detects) sends a signal to tell core to disengage following.
x Whenever following is engaged, the center-here button is highlighted.
x 
x The core keeps track of the position of the ownship icon relative to the center of the
x display. When the display is dragged (or two-finger zoomed, which may also effect a drag),
x that offset is updated. When the ownship position in space is updated, a compensating drag
x is applied to the map to keep the icon at the same location in the viewport.
x 
x The effect is: I click center-here, and the icon snaps to the center of the screen, pulling
x the map along with it. I fly north 200 miles, and the map scrolls under my ship, but the ship
x is "stuck to the glass" of the viewport. I drag a little to "look ahead", so now the
x ownship is at the bottom of the viewport, but tracking is still engaged. I fly another 100
x miles, and more sectional scrolls below my ownship icon, but it's still stuck near
x the bottom of the screen. Then I scroll a state away to peek at an airport. When I do that,
x the center-here button gets un-highlighted, and the map stops moving as the plane moves:
x now the map is focused on whatever I was interested in. I can press "center-here" and the
x map snaps back to put the plane in the middle and starts scrolling as I fly along again.

x During adsb playback, The aircraft heading -- is it coming from adsb? Or subtracting positions?
