# Device Barometric Altimeter

`BARO ft` is offered when the platform reports a pressure sensor. Tapping the
cell opens a core-owned manual altimeter-setting tray. The initial setting is
29.92 inHg; this first version keeps calibration in the session, not the cloud.

NEAREST uses the closest loaded METAR with an altimeter setting, within 100 nm
of ownship and at most 80 minutes old. Missing ownship, unknown/future report
time, missing weather, or no qualifying report disables the button. Core reads
the A#### (inHg) or Q#### (hPa) group, never sea-level pressure from remarks.
The tray shows the station, setting, distance, and age. Eligibility is projected
again on weather/ownship updates and report expiry, and rechecked on every tap.
Applying NEAREST replaces the draft and calibration; it is not continuous auto-set.

Android supplies pressure in hPa and acquisition/receipt timestamps, at most
once per second while the app is foregrounded. Core applies a one-second
exponential pressure filter, standard-atmosphere conversion, and a five-second
sample deadline. Missing/stale pressure produces an empty cell value; the
ordinary session refresh deadline makes that change visible without another tap.
Unsupported platforms do not offer the cell. Web renders the same model but has
no pressure-sensor provider.

Device pressure never populates ownship MSL or aviation pressure altitude.
GPS selection, terrain clearance, traffic, and flight planning do not use it.
The model lives in `SituationController`, whose checkpoints preserve it across
session transactions and nav-db changes. Platforms retain only their native text
edit buffers; core owns validation, calibration, errors, and the tray lifetime.
