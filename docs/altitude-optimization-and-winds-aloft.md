# Altitude Optimization And Winds Aloft

## Goal

Altitude optimization needs a weather model that can answer wind at latitude,
longitude, altitude, and forecast time. The first aircraft-performance model can
come later; this note records the current winds-aloft data source and packaging
experiment.

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

## Current Measuring Package

The first package is intentionally a raw measuring artifact, not a client wire
format. It is published as the live-feed product `winds-aloft`.

Contents:

- `manifest.json`
- `grib2/gfs_<date>_<cycle>_f000.grib2`
- `grib2/gfs_<date>_<cycle>_f003.grib2`
- `grib2/gfs_<date>_<cycle>_f006.grib2`
- `grib2/gfs_<date>_<cycle>_f009.grib2`
- `grib2/gfs_<date>_<cycle>_f012.grib2`

Each GRIB2 file is filtered to:

- Forecast hours: 0, 3, 6, 9, 12
- Pressure levels: 1000, 925, 850, 700, 600, 500, 400, 300 mb
- Variables: UGRD, VGRD, HGT
- Domain: 15N..55N, 135W..50W

UGRD and VGRD are the wind vector components. HGT is included so pressure levels
can be mapped to geometric altitude.

## Size Measurement

The first successful build produced:

- Packaged ZIP: 9.8 MiB
- Unpacked directory: 11 MiB
- Five GRIB2 slices, about 2.1 MiB each

ZIP only saved about 10 percent because GRIB2 is already a packed meteorological
binary format. Future size wins should come from decoding, quantizing, and
chunking the data into an Aerobag-specific wire format, not from stronger outer
compression.

## Next Shape

The current artifact proves the fetch-cache, node-cache, live-feed publishing,
and client transport path. It is not intended for direct client consumption.

Likely next work:

- Decode GRIB2 into an internal structured representation.
- Quantize wind vector and height values to aviation-useful precision.
- Decide whether the client wants route-corridor fetches, tiles, or a compact
  grid bundle.
- Decide forecast horizon and vertical levels based on measured package size and
  altitude optimizer needs.
