# Device Barometric Altimeter

`BARO ft` is offered when the platform reports a pressure sensor. Tapping the
cell opens a core-owned manual altimeter-setting tray. The initial setting is
29.92 inHg; this first version keeps calibration in the session, not the cloud.

NEAREST uses the closest loaded METAR with an altimeter setting, within 100 nm
of ownship and at most 80 minutes old. Missing ownship, unknown/future report
time, missing weather, or no qualifying report disables the button. Core reads
the A#### (inHg) or Q#### (hPa) group, never sea-level pressure from remarks.
The NEAREST button shows the station and report age with the same airport/weather
symbol used by flight-plan buttons. A station without an airport (e.g. KSMP) shows
the weather symbol alone. The setting appears in the input after applying it,
not in a redundant description. Eligibility is projected
again on weather/ownship updates and report expiry, and rechecked on every tap.
Applying NEAREST replaces the draft and calibration; it is not continuous auto-set.

The BARO cell is highlighted when the selected setting differs from that report
by more than 0.10 inHg. Its tray explains the discrepancy; losing eligible weather
clears only this comparison, not the manual altimeter. The tray always cautions:
"BARO ALT from device is cabin alt. Cross-check."

BARO and TGT share one numeric-editor renderer on each platform. Core supplies
the optional heading, accessible input name, unit suffix, action rows, button
labels/symbols, warning, footer, and dismissal action. BARO has no visible heading;
the input is followed by `inHg`. Notices follow the action rows.

Four-digit pressure entry (`3006`) is normalized by core to `30.06`. Explicit
decimal input is unchanged. Core sends a conditional text edit against the exact
raw draft so platforms can preserve the caret without replacing newer typing.
Programmatic replacements (NEAREST and target +/-100) have a separate revision.
The native edit buffer remains local and session commands stay off the UI thread.

Android supplies pressure in hPa and acquisition/receipt timestamps, at most
once per second while the app is foregrounded. Core applies a one-second
exponential pressure filter, standard-atmosphere conversion, and a five-second
sample deadline. Missing/stale pressure produces an empty cell value; the
ordinary session refresh deadline makes that change visible without another tap.
Unsupported platforms do not offer the cell. Web renders the same model but has
no pressure-sensor provider.

Device pressure never populates ownship MSL or aviation pressure altitude.
GPS selection, terrain clearance, traffic, and flight-plan sequencing do not use it.
An explicitly BARO-referenced [altitude intercept advisory](altitude-intercept.md)
uses it, with vertical speed derived from pressure history. Changing the altimeter
setting does not generate a false vertical-speed sample.
The model lives in `SituationController`, whose checkpoints preserve it across
session transactions and nav-db changes. Platforms retain only their native text
edit buffers; core owns validation, calibration, errors, and the tray lifetime.
