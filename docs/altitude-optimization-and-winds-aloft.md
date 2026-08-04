# Altitude Optimization And Winds Aloft

## Goal

Altitude optimization needs an aircraft model and a weather model that can
answer wind and temperature at latitude, longitude, altitude, and forecast
time. This note records the first core planning engine and the current
winds-aloft data contract.

## Core Planner

`app-core` owns the aircraft/profile contract, bounded interpolation, wind
triangle, and trajectory integration. The integrator advances at no more than
1 NM or 30 seconds per step, preserves Flight Plan row IDs in its output, and
produces cumulative ETE and fuel for each row. Cruise, climb, and descent are
separate pointwise performance tables with stable model/profile IDs, versions,
and source metadata.

The Flight Plan wire model distinguishes `basic` estimates from `modeled`
estimates independently of the existing passed/active/planned row tone. Basic
estimates retain today's altitude-independent behavior. Modeled estimates are
all-or-nothing: missing aircraft data, cruise altitude, active-navigation
ownship altitude, destination elevation, forecast coverage, or a required
performance regime leaves the Basic estimate in place and supplies a core-owned
explanation to both UIs. Without active navigation, the plan-origin elevation
is also required; with active navigation, ownship altitude replaces it.

The first sourced aircraft-model components are for the PA46-310P Malibu with a
Continental TSIO-520-BE. Cruise points are a five-point reduction of the N9124Y
Power Settings v3 model, itself sourced from a transcribed POH power table and a
digitized cruise-speed chart. The reduction retains the source curves to within
1 KT from sea level through 24,000 feet:

- 75 percent high speed: 161/175/188/201/213 KTAS at
  0/6,000/12,000/18,000/24,000 feet, 16 GPH at ISA.
- 65 percent economy: 148/162/177/190/202 KTAS, 14 GPH at ISA.
- 55 percent long range: 134/149/164/179/193 KTAS, 12 GPH at ISA.

The initial climb schedule is an explicit rough assumption: 130 KIAS, 36 GPH,
and 1,100 FPM through the same altitude range. The integrator converts its IAS
schedule to TAS from pressure altitude and sampled temperature. Descent is also
an explicit rough assumption: the selected cruise profile's TAS plus 8 KT, the
same fuel flow as cruise, and 500 FPM down. Vertical schedules carry an explicit
indicated-or-true airspeed basis so atmosphere correction cannot accidentally be
applied to the TAS-based descent schedule. These assumptions complete all three
PA46 profiles, but their source metadata preserves which values are POH-derived
and which are rough planning choices.

The first end-to-end UI slice intentionally fixes the selected profile to 65
percent economy and uses 12,000 feet when the plan has no cruise altitude. The
wind control starts at no-wind ISA and, when an atmospheric package is
installed, cycles between `NO WIND` and `FORECAST` through core-issued actions. For
an inactive plan whose first and last rows are airports, core reads their
published elevations, predicts along the materialized route geometry, and
supplies modeled ETE and fuel to the existing Flight Plan cells. Active
navigation with ownship groundspeed continues to use the familiar groundspeed
extrapolation instead and reports `MODE GS`; altitude comparison always uses the
selected planning atmosphere.

## Source Chosen

Use NOAA/NCEP GFS 0.25 degree forecast data through the NOMADS GRIB filter.

The 0.25 degree value is the horizontal sample density: roughly one sample every
15 nautical miles in latitude, with longitude spacing shrinking by cosine of
latitude. This is appropriate for route-scale planning, not airport-scale local
effects.

GFS runs four cycles per day: 00, 06, 12, and 18 UTC. The current preprocessor
prototype selects a conservative cycle by subtracting nine hours from build time
and rounding down to the previous six-hour model cycle. This avoids asking
NOMADS for a cycle or early forecast file that has not landed yet.

References:

- https://www.ncei.noaa.gov/products/weather-climate-models/global-forecast
- https://nomads.ncep.noaa.gov/
- https://nomads.ncep.noaa.gov/gribfilter.php?ds=gfs_0p25

## Atmospheric NavKv Package

Live-feeds decodes GRIB2 with GDAL and publishes `winds-aloft` as a NavKv
package. GRIB2 and temporary decoded rasters remain build inputs; they are not
copied into the published state. Its durable members are only:

- `manifest.json`
- `root`
- `page_####`

Each GRIB2 file is filtered to:

- Forecast hours: 0, 3, 6, 9, 12
- Pressure levels: 1000, 925, 850, 700, 600, 500, 400, 300 mb
- Variables: UGRD, VGRD, HGT, TMP
- Domain: 15N..55N, 135W..50W

UGRD and VGRD are the wind vector components. HGT is included so pressure levels
can be mapped to geometric altitude. TMP supplies temperature at each pressure
level for performance and density calculations.

The 0.25-degree source grid is split into 8-by-8 spatial tiles addressed as
`atmosphere/tile/r#####/c#####`. Every tile contains all five forecast times and
all eight pressure levels in `valid_time,pressure_level,row,column` order. Wind
is quantized to 0.1 m/s, temperature to 0.01 C, and geopotential height to one
meter. The packed little-endian integer arrays and validity mask are wrapped in
a versioned protobuf message. The package manifest owns the axes, units, grid
origin and spacing, tile dimensions, and protobuf contract identifier.

Core validates and retains the installed NavKv state and samples it with
bilinear spatial interpolation, linear interpolation between geopotential
heights, and linear interpolation between forecast times. Masked pressure
surfaces below terrain are skipped, while altitudes beyond the remaining valid
surfaces retain the nearest edge value rather than extrapolating. If selected
GFS data does not cover the route, altitude, or forecast time, core retains the
basic estimate and reports a wind-model availability reason; the wind control
remains available so the user can return to no-wind planning.

## Size Measurement

A build from current NOAA inputs produced 903 spatial tiles, 275 64-KiB NavKv
pages, and 17,754,086 uncompressed value bytes. The live-feed package transport
xz-compresses NavKv pages. Forecast cycles intentionally publish full snapshots:
nearly every atmospheric tile changes, so a NavKv delta would approximate the
full state while adding mutation overhead.
