# Altitude Intercept Advisory

Implemented behavior:

- Add a target-altitude banner cell and core-owned numeric editor, with 100-ft
  steps, explicit GPS/BARO reference selection, and Off. Share the editor renderer
  with the existing altimeter-setting tray.
- Predict time and distance using actual vertical speed and ground speed. Draw a
  geographic arc around the current ground track, independent of the flight plan.
  Orient the label's top along that track, using the shared map rotation rather
  than keeping the text upright relative to the screen.
  Hide it for stale/invalid inputs, insufficient vertical trend, motion away from
  the target altitude, and within the target capture band.
- Keep the target reference fixed until explicitly changed. Never interpret raw
  29.92 pressure altitude as GPS MSL or switch a BARO target to GPS on sensor loss.
- Device BARO remains an advisory cabin-pressure measurement, separate from
  ownship GPS, terrain clearance, and traffic. Its tray says: "BARO ALT from
  device is cabin alt. Cross-check." Cross-check with the aircraft altimeter;
  no weather service is required for a manually set altimeter.
- For BARO targets, provide a yellow one-minute approach cue, with brief blinking
  on entry and hysteresis. Core owns timing and presentation state.
- On the BARO cell, flag a selected-setting discrepancy greater than 0.10 inHg
  against the nearest eligible METAR. Existing distance/age limits apply. Show
  the reason in the setting tray; never automatically change the setting. Missing
  weather removes only the comparison warning, not the altitude prediction.
- Altitude reference, command validation, labels, warning reasons, prediction,
  geometry, and deadlines belong in core. Platforms render generated models and
  send commands. Geographic geometry uses the shared map-frame machinery.
- Dismiss the shared editor by tapping outside or pressing Enter/IME Done. There
  is no separate Close button. Disabled controls retain disabled styling and use
  core-supplied reasons through the standard hover/tap-help mechanism.
- Opening selects the numeric field; typing stays in a responsive local edit
  buffer while Android sends ordered commands through its background runner.
  The tray fits above the keyboard and scrolls within that available space.
  Sensor observations and editor commands never write the unrelated settings
  document: these altitude values are session-local.

Verification: deterministic core tests for climb/descent distance, wrong-way and
level suppression, source loss/change, pressure-setting changes, stale fixes,
noisy GPS, threshold hysteresis and timed blinking, METAR expiry and discrepancy
boundaries. UI component tests exercise real taps, numeric entry, target controls,
warning text, and shared-frame arc projection. Run the cheap cross-platform
preflight and inspect the running web experience; report unrun device checks.

## Limits

The target is session-local and initially off. The first edit chooses device BARO
when a sensor is available, otherwise GPS; subsequent sensor loss never changes
an explicit reference. This does not ingest an aircraft's calibrated ADC altitude.
GPS predictions use only MSL-altitude history, never raw 29.92 pressure altitude
or an unqualified external VSI. Missing MSL samples and source changes break the
trend. Gross vertical uncertainty (over 50 m) suppresses GPS prediction.

Both references require at least 100 fpm toward the target, 5 kt ground speed,
fresh position and ground track. The arc disappears within 100 ft of target and
for predictions over an hour or 200 nm away. It spans 30 degrees centered on
ground track. This advisory is not altitude-hold guidance or a terrain guarantee.

The BARO approach cue enters at 60 seconds and releases above 75 seconds (or
when prediction becomes unavailable). It blinks at 500-ms half-periods for six
seconds, then stays yellow. Values remain readable throughout. All deadlines and
highlight states are core projections, not independently running UI timers.
